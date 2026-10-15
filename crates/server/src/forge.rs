//! Tasks from GitHub, GitLab and Gitea/Forgejo (7.6, D-97). Accounts connect with an access
//! token (stored encrypted, secrets.rs). Every [`INTERVAL`] the job asks each forge what is
//! yours (assigned issues, assigned PRs/MRs, review requests, mentions; each switchable)
//! and makes a task per new item, in the project its repository rule names (else the
//! inbox). A task whose item has gone (closed, merged, no longer yours) is completed.
//! Read-only towards the forge. On failure nothing changes and the account shows the error.

use std::{sync::LazyLock, time::Duration};

use anyhow::{Context, bail};
use chrono::{SecondsFormat, Utc};
use reqwest::{StatusCode, Url};
use serde_json::{Value, json};
use streamline_domain::{
    forge::{self, ISSUE, Item, Known, MENTION, PR, REVIEW},
    order::key_after,
};

use crate::{
    AppState,
    auth::AuthUser,
    db::next_rev,
    events::Change,
    models::{ForgeAccount, Task, User, upsert_task},
    ownership::Owner,
    util::{new_id, now},
};

/// How often accounts are polled in the background.
pub const INTERVAL: Duration = Duration::from_secs(10 * 60);
pub const KINDS: [&str; 3] = ["github", "gitlab", "gitea"];
/// Limits against a huge backlog or a misbehaving server.
const MAX_BODY: usize = 5 << 20;
const MAX_PAGES: u32 = 3;
const MAX_NEW_PER_RUN: usize = 100;

/// Polls run one at a time.
static LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(Default::default);

pub fn kind_name(kind: &str) -> &'static str {
    match kind {
        "github" => "GitHub",
        "gitlab" => "GitLab",
        _ => "Gitea",
    }
}

/// The server's web address as saved: `https://github.com` etc. when left empty, no
/// trailing slash, no credentials or query.
pub fn normalize_url(kind: &str, input: &str) -> anyhow::Result<String> {
    let input = input.trim().trim_end_matches('/');
    let input = match (kind, input) {
        ("github", "") => "https://github.com",
        ("gitlab", "") => "https://gitlab.com",
        ("gitea", "") => bail!("enter your Gitea or Forgejo server's address"),
        (_, s) => s,
    };
    let url = Url::parse(input).context("that isn't a web address")?;
    crate::net::check_url(&url)?;
    if !url.username().is_empty() || url.password().is_some() {
        bail!("put the token in the token field, not in the address");
    }
    if url.query().is_some() || url.fragment().is_some() {
        bail!("enter just the server's address");
    }
    if input.len() > 500 {
        bail!("the address is too long");
    }
    Ok(input.to_string())
}

/// Where the REST API lives for a server's web address.
fn api_base(kind: &str, url: &str) -> anyhow::Result<Url> {
    let parsed = Url::parse(url)?;
    let base = match (kind, parsed.host_str()) {
        ("github", Some("github.com" | "www.github.com" | "api.github.com")) => {
            "https://api.github.com/".to_string()
        }
        // GitHub Enterprise Server.
        ("github", _) => format!("{url}/api/v3/"),
        ("gitlab", _) => format!("{url}/api/v4/"),
        _ => format!("{url}/api/v1/"),
    };
    Ok(Url::parse(&base)?)
}

pub struct Forge {
    kind: String,
    api: Url,
    token: String,
    http: reqwest::Client,
}

impl Forge {
    pub fn new(kind: &str, url: &str, token: &str) -> anyhow::Result<Self> {
        Ok(Self {
            kind: kind.into(),
            api: api_base(kind, url)?,
            token: token.into(),
            http: crate::net::client(30, false)?,
        })
    }

    pub fn for_account(state: &AppState, a: &ForgeAccount) -> anyhow::Result<Self> {
        let token = a
            .secret
            .as_deref()
            .map(|s| state.secrets.decrypt(s))
            .transpose()?
            .unwrap_or_default();
        Self::new(&a.kind, &a.url, &token)
    }

    fn url(&self, path: &str, query: &[(&str, &str)]) -> anyhow::Result<Url> {
        let mut u = self.api.join(path)?;
        if !query.is_empty() {
            u.query_pairs_mut().extend_pairs(query);
        }
        Ok(u)
    }

    async fn get(&self, url: Url) -> anyhow::Result<Value> {
        let auth = match self.kind.as_str() {
            "github" => ("Authorization", format!("Bearer {}", self.token)),
            "gitlab" => ("PRIVATE-TOKEN", self.token.clone()),
            _ => ("Authorization", format!("token {}", self.token)),
        };
        let res = self
            .http
            .get(url)
            .header(auth.0, auth.1)
            .header("Accept", "application/json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .context("can't reach the server")?;
        let status = res.status();
        if status == StatusCode::UNAUTHORIZED {
            bail!("the token was refused (expired or revoked?)");
        }
        if status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS {
            if res
                .headers()
                .get("x-ratelimit-remaining")
                .is_some_and(|v| v == "0")
                || status == StatusCode::TOO_MANY_REQUESTS
            {
                bail!("the server's rate limit was reached; trying again later");
            }
            bail!("the token isn't allowed to read this (check its permissions)");
        }
        if !status.is_success() {
            bail!("the server answered {status}");
        }
        let body = crate::net::text_limited(res, MAX_BODY).await?;
        serde_json::from_str(&body)
            .context("the server's answer isn't JSON (is the address right?)")
    }

    /// Whose token this is (checks the token works).
    pub async fn login(&self) -> anyhow::Result<String> {
        let me = self.get(self.url("user", &[])?).await?;
        let field = if self.kind == "gitlab" {
            "username"
        } else {
            "login"
        };
        me[field]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(String::from)
            .context("the server didn't say whose token this is (is the address right?)")
    }

    /// The open items that are yours for the reasons in `kinds`. The flag says whether the
    /// answers were complete (nothing cut off).
    pub async fn items(&self, login: &str, kinds: u8) -> anyhow::Result<(Vec<Item>, bool)> {
        let mut all = vec![];
        let mut whole = true;
        for kind in [ISSUE, PR, REVIEW, MENTION] {
            if kinds & kind == 0 {
                continue;
            }
            let (items, complete) = match self.kind.as_str() {
                "github" => self.github(login, kind).await?,
                "gitlab" => self.gitlab(login, kind).await?,
                _ => self.gitea(kind).await?,
            };
            whole &= complete;
            all.extend(items);
        }
        Ok((forge::merge(all), whole))
    }

    async fn github(&self, login: &str, kind: u8) -> anyhow::Result<(Vec<Item>, bool)> {
        let q = match kind {
            ISSUE => format!("is:open is:issue archived:false assignee:{login}"),
            PR => format!("is:open is:pr archived:false assignee:{login}"),
            REVIEW => format!("is:open is:pr archived:false user-review-requested:{login}"),
            _ => format!("is:open archived:false mentions:{login}"),
        };
        let mut out = vec![];
        for page in 1..=MAX_PAGES {
            let page = page.to_string();
            let v = self
                .get(self.url(
                    "search/issues",
                    &[("q", &q), ("per_page", "100"), ("page", &page)],
                )?)
                .await?;
            let list = v["items"]
                .as_array()
                .context("unexpected answer from GitHub")?;
            for it in list {
                let repo = it["repository_url"]
                    .as_str()
                    .and_then(|u| u.split_once("/repos/"))
                    .map(|(_, r)| r.to_string());
                if let (Some(repo), Some(n)) = (repo, it["number"].as_u64()) {
                    out.push(item(&repo, &format!("{repo}#{n}"), it, "html_url", kind));
                }
            }
            let total = v["total_count"].as_u64().unwrap_or(0) as usize;
            if v["incomplete_results"].as_bool() == Some(true) {
                return Ok((out, false));
            }
            if list.len() < 100 || out.len() >= total {
                return Ok((out, true));
            }
        }
        Ok((out, false))
    }

    async fn gitlab(&self, login: &str, kind: u8) -> anyhow::Result<(Vec<Item>, bool)> {
        if kind == MENTION {
            // Mentions are only found as to-dos (pending, on open items).
            let (a, wa) = self.gitlab_todos("mentioned").await?;
            let (b, wb) = self.gitlab_todos("directly_addressed").await?;
            return Ok((a.into_iter().chain(b).collect(), wa && wb));
        }
        let (path, query): (&str, Vec<(&str, &str)>) = match kind {
            ISSUE => ("issues", vec![("scope", "assigned_to_me")]),
            PR => ("merge_requests", vec![("scope", "assigned_to_me")]),
            _ => (
                "merge_requests",
                vec![("scope", "all"), ("reviewer_username", login)],
            ),
        };
        self.gitlab_pages(path, &query, |v| gitlab_item(v, kind))
            .await
    }

    async fn gitlab_todos(&self, action: &str) -> anyhow::Result<(Vec<Item>, bool)> {
        self.gitlab_pages("todos", &[("state", "pending"), ("action", action)], |t| {
            matches!(t["target_type"].as_str(), Some("Issue" | "MergeRequest"))
                .then(|| &t["target"])
                .filter(|target| target["state"] == "opened")
                .and_then(|target| gitlab_item(target, MENTION))
        })
        .await
    }

    async fn gitlab_pages(
        &self,
        path: &str,
        query: &[(&str, &str)],
        parse: impl Fn(&Value) -> Option<Item>,
    ) -> anyhow::Result<(Vec<Item>, bool)> {
        let mut out = vec![];
        for page in 1..=MAX_PAGES {
            let page = page.to_string();
            let mut q = query.to_vec();
            if path != "todos" {
                q.push(("state", "opened"));
            }
            q.extend([("per_page", "100"), ("page", page.as_str())]);
            let v = self.get(self.url(path, &q)?).await?;
            let list = v.as_array().context("unexpected answer from GitLab")?;
            out.extend(list.iter().filter_map(&parse));
            if list.len() < 100 {
                return Ok((out, true));
            }
        }
        Ok((out, false))
    }

    async fn gitea(&self, kind: u8) -> anyhow::Result<(Vec<Item>, bool)> {
        let filter: &[(&str, &str)] = match kind {
            ISSUE => &[("type", "issues"), ("assigned", "true")],
            PR => &[("type", "pulls"), ("assigned", "true")],
            REVIEW => &[("type", "pulls"), ("review_requested", "true")],
            _ => &[("mentioned", "true")],
        };
        let mut out = vec![];
        for page in 1..=MAX_PAGES {
            let page = page.to_string();
            let mut q = filter.to_vec();
            q.extend([("state", "open"), ("limit", "50"), ("page", page.as_str())]);
            let v = self.get(self.url("repos/issues/search", &q)?).await?;
            let list = v.as_array().context("unexpected answer from the server")?;
            for it in list {
                if let (Some(repo), Some(n)) = (
                    it["repository"]["full_name"].as_str(),
                    it["number"].as_u64(),
                ) {
                    out.push(item(repo, &format!("{repo}#{n}"), it, "html_url", kind));
                }
            }
            if list.len() < 50 {
                return Ok((out, true));
            }
        }
        Ok((out, false))
    }
}

fn item(repo: &str, key: &str, v: &Value, url_field: &str, kind: u8) -> Item {
    Item {
        key: key.into(),
        repo: repo.into(),
        title: v["title"].as_str().unwrap_or("(untitled)").into(),
        url: v[url_field].as_str().unwrap_or_default().into(),
        kinds: kind,
    }
}

/// A GitLab issue or MR: `references.full` is `group/proj#12` or `group/proj!3`.
fn gitlab_item(v: &Value, kind: u8) -> Option<Item> {
    let key = v["references"]["full"].as_str()?;
    let repo = &key[..key.rfind(['#', '!'])?];
    Some(item(repo, key, v, "web_url", kind))
}

/// Poll one account now and record the outcome on it.
pub async fn sync_account(state: &AppState, account_id: &str) -> anyhow::Result<ForgeAccount> {
    let _guard = LOCK.lock().await;
    let account: ForgeAccount = sqlx::query_as("SELECT * FROM forge_accounts WHERE id = ?")
        .bind(account_id)
        .fetch_one(&state.db.read)
        .await?;
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(&account.user_id)
        .fetch_one(&state.db.read)
        .await?;
    let result = run(state, &account, &user).await;
    if let Err(e) = &result {
        tracing::info!("forge sync failed for account {}: {e:#}", account.id);
    }
    let (status, error) = match &result {
        Ok(()) => ("ok", None),
        Err(e) => ("error", Some(format!("{e:#}"))),
    };
    let ts = now();
    sqlx::query("UPDATE forge_accounts SET status = ?, last_error = ?, last_sync_at = ?, updated_at = ? WHERE id = ?")
        .bind(status)
        .bind(&error)
        .bind(&ts)
        .bind(&ts)
        .bind(&account.id)
        .execute(&state.db.write)
        .await?;
    Ok(sqlx::query_as("SELECT * FROM forge_accounts WHERE id = ?")
        .bind(&account.id)
        .fetch_one(&state.db.read)
        .await?)
}

async fn run(state: &AppState, a: &ForgeAccount, user: &User) -> anyhow::Result<()> {
    let enabled = a.kinds();
    let (items, whole) = Forge::for_account(state, a)?
        .items(&a.login, enabled)
        .await?;
    let rows: Vec<(String, i64, String, bool)> = sqlx::query_as(
        "SELECT f.item_key, f.kinds, f.task_id,
                COALESCE(t.status = 'open' AND t.deleted_at IS NULL, 0)
         FROM forge_items f LEFT JOIN tasks t ON t.id = f.task_id WHERE f.account_id = ?",
    )
    .bind(&a.id)
    .fetch_all(&state.db.read)
    .await?;
    let known: Vec<Known> = rows
        .iter()
        .map(|(key, kinds, _, open)| Known {
            key: key.clone(),
            kinds: *kinds as u8,
            open: *open,
        })
        .collect();
    let plan = forge::reconcile(&items, &known, enabled, whole);
    let me = AuthUser {
        user: user.clone(),
        session_hash: None,
        token_hash: None,
        groups: crate::auth::groups_of(state, &user.id).await?,
    };

    let mut tx = state.db.write.begin().await?;
    let mut changes = vec![];
    for &i in plan.create.iter().take(MAX_NEW_PER_RUN) {
        let t = create_task(&mut tx, a, &me, &items[i]).await?;
        sqlx::query(
            "INSERT INTO forge_items (account_id, item_key, task_id, kinds) VALUES (?, ?, ?, ?)",
        )
        .bind(&a.id)
        .bind(&items[i].key)
        .bind(&t.id)
        .bind(items[i].kinds as i64)
        .execute(&mut *tx)
        .await?;
        changes.push(Change::task(&t));
    }
    for (key, kinds) in &plan.reasons {
        sqlx::query("UPDATE forge_items SET kinds = ? WHERE account_id = ? AND item_key = ?")
            .bind(*kinds as i64)
            .bind(&a.id)
            .bind(key)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    state.bus.publish(changes);

    // Done upstream: the usual completion (dependents, live updates), as the user.
    for key in &plan.complete {
        let Some((_, _, task_id, _)) = rows.iter().find(|r| &r.0 == key) else {
            continue;
        };
        if let Err(e) = crate::routes::tasks::patch(
            axum::extract::State(state.clone()),
            AuthUser {
                user: me.user.clone(),
                session_hash: None,
                token_hash: None,
                groups: me.groups.clone(),
            },
            axum::extract::Path(task_id.clone()),
            axum::Json(json!({"status": "done"})),
        )
        .await
        {
            tracing::info!("couldn't complete task {task_id} from {key}: {e:?}");
        }
    }
    Ok(())
}

async fn create_task(
    conn: &mut sqlx::SqliteConnection,
    a: &ForgeAccount,
    me: &AuthUser,
    it: &Item,
) -> anyhow::Result<Task> {
    // The rule's project, if it still exists and is yours to see; else the inbox.
    let project = match forge::project_for(&a.rules, &it.repo) {
        Some(p) => crate::routes::projects::load_visible(conn, me, p)
            .await
            .ok(),
        None => None,
    };
    let owner = project
        .as_ref()
        .map(Owner::of_project)
        .unwrap_or_else(|| Owner::me(me));
    let project_id = project.as_ref().map(|p| p.id.clone());
    let last: Option<String> = sqlx::query_scalar(
        "SELECT MAX(position) FROM tasks WHERE project_id IS ?1 AND (?1 IS NOT NULL OR owner_user_id = ?2) AND deleted_at IS NULL",
    )
    .bind(&project_id)
    .bind(me.id())
    .fetch_one(&mut *conn)
    .await?;
    let title: String = forge::task_title(it).chars().take(500).collect();
    let ts = now();
    let t = Task {
        id: new_id(),
        owner_user_id: owner.user,
        owner_group_id: owner.group,
        assignee_user_id: None,
        project_id,
        title,
        notes: format!(
            "{} on {}: {}",
            it.key,
            kind_name(&a.kind),
            forge::reasons_text(it.kinds)
        ),
        status: "open".into(),
        position: key_after(last.as_deref()),
        due_date: None,
        estimate_min: None,
        difficulty: None,
        importance: None,
        urgency: None,
        actual_min: 0,
        task_type_id: crate::routes::tasks::DEFAULT_TASK_TYPE.into(),
        carry_count: 0,
        started_at: None,
        completed_at: None,
        completed_by: None,
        ext_source: Some("forge".into()),
        ext_id: Some(format!("{}:{}", a.id, it.key)),
        ext_url: (!it.url.is_empty()).then(|| it.url.clone()),
        place_id: project.as_ref().and_then(|p| p.default_place_id.clone()),
        also_project_ids: sqlx::types::Json(vec![]),
        depends_on: sqlx::types::Json(vec![]),
        blocked: false,
        wait_min: None,
        ready_at: None,
        workflow_instance_id: None,
        workflow_step: None,
        workflow_steps: None,
        series_id: None,
        occurrence_key: None,
        occurrence_date: None,
        event_id: None,
        window_end: None,
        waiting_since: None,
        check_back_at: None,
        waiting_note: String::new(),
        waiting_by: None,
        checklist: sqlx::types::Json(vec![]),
        created_at: ts.clone(),
        updated_at: ts,
        deleted_at: None,
        rev: next_rev(conn).await?,
    };
    upsert_task(conn, &t).await?;
    Ok(t)
}

/// Background job: poll accounts whose last poll is older than [`INTERVAL`].
pub async fn run_due(state: &AppState) -> anyhow::Result<()> {
    let cutoff = (Utc::now() - chrono::Duration::from_std(INTERVAL)?)
        .to_rfc3339_opts(SecondsFormat::Millis, true);
    let due: Vec<String> = sqlx::query_scalar(
        "SELECT a.id FROM forge_accounts a JOIN users u ON u.id = a.user_id
         WHERE u.deleted_at IS NULL AND (a.last_sync_at IS NULL OR a.last_sync_at < ?)",
    )
    .bind(cutoff)
    .fetch_all(&state.db.read)
    .await?;
    for id in due {
        sync_account(state, &id).await?;
    }
    Ok(())
}

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let s = state.clone();
            crate::util::guarded("forge sync", async move {
                if let Err(e) = run_due(&s).await {
                    tracing::warn!("forge sync job failed: {e:#}");
                }
            })
            .await;
        }
    });
}
