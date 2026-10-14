//! The desktop app's local replica and outbox (D-78, D-79). Every request from the page
//! goes through here:
//!
//! - Reads are sent to the server; `/sync` answers (and the live `change` events) also
//!   feed a SQLite replica of what the user can see. When the server can't be reached,
//!   `/sync` and `/today` are answered from the replica, so the app opens and works offline.
//! - Writes the page made optimistically (it says which records it changed and how they
//!   look now) are kept in an outbox when the server can't be reached, or while earlier
//!   ones are still waiting, and answered with the page's own result. The outbox is sent
//!   in order once the server is back, each edit with the values it was based on (from the
//!   replica's last server copy) and when it was made, so the server merges it (9.3).
//!   Other writes need a connection, as in the browser.
//!
//! The replica is a cache of the server, never a second database: rollover, routines and
//! reminders run only on the server.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::{Map, Value, json};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use crate::remote;

/// A record a write changed: (kind as the page names it, id, the record afterwards or
/// `None` when deleted).
pub type Touched = (String, String, Option<Value>);

#[derive(Debug, Clone, Serialize)]
pub struct ApiResponse {
    pub status: u16,
    pub body: Value,
    /// The server couldn't be reached.
    pub offline: bool,
}

/// What the outbox did, for the page's banner and its "synced with changes" notice.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    /// Changes still waiting.
    pub pending: i64,
    /// Changes the server took in this round.
    pub sent: usize,
    /// Fields both sides had changed: `{kind, id, field, item, kept, server, client}`.
    pub conflicts: Vec<Value>,
    /// Changes the server refused (deleted meanwhile, no longer valid): `{kind, id, method, status, message}`.
    pub dropped: Vec<Value>,
}

/// Where each kind lives in a `/sync` answer.
pub fn sync_key(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "task" => "tasks",
        "project" => "projects",
        "day_entry" => "day_entries",
        "day_plan" => "day_plans",
        "focus_session" => "focus_sessions",
        "series" => "series",
        "place" => "places",
        "workflow" => "workflows",
        "task_template" => "task_templates",
        "group" => "groups",
        "calendar" => "calendars",
        "event" => "events",
        "event_project" => "event_projects",
        "person" => "people",
        "day_template" => "day_templates",
        "time_block" => "time_blocks",
        "day_record" => "day_records",
        "health_day" => "health_days",
        "metric" => "metrics",
        "metric_entry" => "metric_entries",
        "goal" => "goals",
        "occasion_template" => "occasion_templates",
        _ => return None,
    })
}

/// Lists a `/sync` answer always sends whole, even when only changes were asked for.
const ALWAYS_COMPLETE: [&str; 2] = ["groups", "occasion_templates"];

/// Writes that may wait in the outbox: creating, editing and deleting the records people
/// change day to day (the ones the server merges, D-85). Anything else needs a connection.
pub fn queueable(method: &str, path: &str) -> bool {
    let path = path.split('?').next().unwrap_or("");
    let s: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    matches!(
        (method, s.as_slice()),
        ("POST", ["tasks" | "projects" | "series"])
            | (
                "PATCH" | "DELETE",
                [
                    "tasks" | "projects" | "series" | "day-entries" | "metric-entries",
                    _
                ]
            )
            | ("POST", ["days", _, "entries"])
            | ("PUT", ["days", _, "record" | "health"])
            | ("DELETE", ["days", _, "health"])
            | ("POST", ["metrics", _, "entries"])
    )
}

fn record_id(v: &Value) -> Option<String> {
    ["id", "kind"]
        .iter()
        .find_map(|k| v.get(*k)?.as_str().map(String::from))
}

fn now_ms() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// No answer from the server at all (as opposed to an error answer).
struct Unreachable;

pub struct Engine {
    db: SqlitePool,
    creds: Mutex<Option<(String, String)>>,
    replaying: tokio::sync::Mutex<()>,
    on_report: Box<dyn Fn(&Report) + Send + Sync>,
}

impl Engine {
    pub async fn open(
        path: &Path,
        on_report: impl Fn(&Report) + Send + Sync + 'static,
    ) -> Result<Arc<Self>, String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));
        let db = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await
            .map_err(|e| e.to_string())?;
        for sql in [
            "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            // The server's records as last seen.
            "CREATE TABLE IF NOT EXISTS records (key TEXT NOT NULL, id TEXT NOT NULL, json TEXT NOT NULL, PRIMARY KEY (key, id))",
            // The page's view of records with waiting changes (json NULL = deleted).
            "CREATE TABLE IF NOT EXISTS overlay (key TEXT NOT NULL, id TEXT NOT NULL, json TEXT, PRIMARY KEY (key, id))",
            "CREATE TABLE IF NOT EXISTS outbox (seq INTEGER PRIMARY KEY AUTOINCREMENT, method TEXT NOT NULL, path TEXT NOT NULL,
               body TEXT, kind TEXT NOT NULL, id TEXT NOT NULL, created_at TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS outbox_touch (seq INTEGER NOT NULL, key TEXT NOT NULL, id TEXT NOT NULL)",
        ] {
            sqlx::query(sql).execute(&db).await.map_err(|e| e.to_string())?;
        }
        Ok(Arc::new(Self {
            db,
            creds: Mutex::new(None),
            replaying: tokio::sync::Mutex::new(()),
            on_report: Box::new(on_report),
        }))
    }

    pub fn set_credentials(&self, creds: Option<(String, String)>) {
        *self.creds.lock().unwrap() = creds;
    }

    /// Before signing in to `server`: a replica of another server (or account) is dropped.
    pub async fn reset_for(&self, server: &str) -> Result<(), String> {
        let current: Option<String> = self.meta("server").await;
        if current.as_deref() != Some(server) {
            self.clear().await?;
            self.set_meta("server", server).await;
        }
        Ok(())
    }

    /// Forget everything (signing out): the replica and any changes still waiting.
    pub async fn clear(&self) -> Result<(), String> {
        for t in ["meta", "records", "overlay", "outbox", "outbox_touch"] {
            sqlx::query(&format!("DELETE FROM {t}"))
                .execute(&self.db)
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// A value `/sync` sends once (`me`, `focus_timer`…) as last seen.
    pub async fn single(&self, key: &str) -> Option<Value> {
        self.meta(&format!("single:{key}"))
            .await
            .and_then(|v| serde_json::from_str(&v).ok())
    }

    async fn meta(&self, key: &str) -> Option<String> {
        sqlx::query_scalar("SELECT value FROM meta WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.db)
            .await
            .ok()
            .flatten()
    }

    async fn set_meta(&self, key: &str, value: &str) {
        let _ = sqlx::query("INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2")
            .bind(key)
            .bind(value)
            .execute(&self.db)
            .await;
    }

    pub async fn pending(&self) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM outbox")
            .fetch_one(&self.db)
            .await
            .unwrap_or(0)
    }

    async fn send(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<ApiResponse, Unreachable> {
        let Some((server, token)) = self.creds.lock().unwrap().clone() else {
            return Ok(ApiResponse {
                status: 401,
                body: json!({"title": "Not signed in"}),
                offline: false,
            });
        };
        let url = remote::api_url(&server, path).map_err(|_| Unreachable)?;
        let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|_| Unreachable)?;
        let mut req = remote::client(Some(Duration::from_secs(60)))
            .request(method, url)
            .bearer_auth(token);
        if let Some(b) = body {
            req = req.json(b);
        }
        let res = req.send().await.map_err(|_| Unreachable)?;
        let status = res.status().as_u16();
        let bytes = remote::read_limited(res).await.map_err(|_| Unreachable)?;
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        Ok(ApiResponse {
            status,
            body,
            offline: false,
        })
    }

    /// A request from the page (see the module docs).
    pub async fn request(
        self: &Arc<Self>,
        method: &str,
        path: &str,
        body: Option<Value>,
        touched: Vec<Touched>,
    ) -> ApiResponse {
        if method == "GET" {
            return match self.send(method, path, None).await {
                Ok(r) => {
                    if is_sync(path) && r.status == 200 {
                        self.ingest(&r.body).await;
                        self.kick();
                        return ApiResponse {
                            body: self.with_overlay(r.body).await,
                            ..r
                        };
                    }
                    r
                }
                Err(Unreachable) => self.answer_offline(path).await,
            };
        }
        let can_wait = !touched.is_empty() && queueable(method, path);
        if self.pending().await > 0 {
            if can_wait {
                let r = self.enqueue(method, path, body, &touched, false).await;
                self.kick();
                return r;
            }
            // Other writes go after the waiting ones, or not at all.
            self.replay().await;
            if self.pending().await > 0 {
                return offline();
            }
        }
        match self.send(method, path, body.as_ref()).await {
            Ok(r) => {
                if (200..300).contains(&r.status) {
                    self.remember_answer(&touched, &r.body).await;
                }
                r
            }
            Err(Unreachable) if can_wait => self.enqueue(method, path, body, &touched, true).await,
            Err(Unreachable) => offline(),
        }
    }

    /// The page's own result stands in for the server's answer; the edit waits.
    async fn enqueue(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        touched: &[Touched],
        unreachable: bool,
    ) -> ApiResponse {
        // The record the request is about: the one named in the path, else the first.
        let segments: Vec<&str> = path.split('?').next().unwrap_or("").split('/').collect();
        let target = touched
            .iter()
            .find(|(_, id, _)| segments.contains(&id.as_str()))
            .or_else(|| touched.first());
        let (kind, id) = target
            .map(|(k, i, _)| (k.clone(), i.clone()))
            .unwrap_or_default();
        let mut body = body;
        if let (Some(Value::Object(fields)), Some(key)) = (body.as_mut(), sync_key(&kind)) {
            fields.insert("edited_at".into(), json!(now_ms()));
            if matches!(method, "PATCH" | "PUT")
                && let Some(Value::Object(server)) = self.record(key, &id).await
            {
                let base: Map<String, Value> = fields
                    .keys()
                    .filter(|k| !matches!(k.as_str(), "edited_at" | "base"))
                    .map(|k| (k.clone(), server.get(k).cloned().unwrap_or(Value::Null)))
                    .collect();
                fields.insert("base".into(), Value::Object(base));
            }
        }
        let mut tx = match self.db.begin().await {
            Ok(tx) => tx,
            Err(_) => return offline(),
        };
        let seq: i64 = match sqlx::query_scalar(
            "INSERT INTO outbox (method, path, body, kind, id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING seq",
        )
        .bind(method)
        .bind(path)
        .bind(body.as_ref().map(|b| b.to_string()))
        .bind(&kind)
        .bind(&id)
        .bind(now_ms())
        .fetch_one(&mut *tx)
        .await
        {
            Ok(s) => s,
            Err(_) => return offline(),
        };
        for (k, i, after) in touched {
            let Some(key) = sync_key(k) else { continue };
            let _ = sqlx::query("INSERT INTO outbox_touch (seq, key, id) VALUES (?1, ?2, ?3)")
                .bind(seq)
                .bind(key)
                .bind(i)
                .execute(&mut *tx)
                .await;
            let _ = sqlx::query("INSERT INTO overlay (key, id, json) VALUES (?1, ?2, ?3) ON CONFLICT(key, id) DO UPDATE SET json = ?3")
                .bind(key)
                .bind(i)
                .bind(after.as_ref().map(|v| v.to_string()))
                .execute(&mut *tx)
                .await;
        }
        if tx.commit().await.is_err() {
            return offline();
        }
        let answer = target
            .and_then(|(_, _, after)| after.clone())
            .unwrap_or(Value::Null);
        (self.on_report)(&Report {
            pending: self.pending().await,
            ..Default::default()
        });
        ApiResponse {
            status: if method == "DELETE" { 204 } else { 200 },
            body: answer,
            offline: unreachable,
        }
    }

    /// Send what's waiting, in order, until done or the server can't be reached.
    pub async fn replay(&self) -> Report {
        let Ok(_guard) = self.replaying.try_lock() else {
            return Report {
                pending: self.pending().await,
                ..Default::default()
            };
        };
        let mut report = Report::default();
        loop {
            let row = sqlx::query(
                "SELECT seq, method, path, body, kind, id FROM outbox ORDER BY seq LIMIT 1",
            )
            .fetch_optional(&self.db)
            .await
            .ok()
            .flatten();
            let Some(row) = row else { break };
            let seq: i64 = row.get("seq");
            let method: String = row.get("method");
            let path: String = row.get("path");
            let kind: String = row.get("kind");
            let id: String = row.get("id");
            let body: Option<Value> = row
                .get::<Option<String>, _>("body")
                .and_then(|b| serde_json::from_str(&b).ok());
            let Ok(r) = self.send(&method, &path, body.as_ref()).await else {
                break;
            };
            match r.status {
                200..=299 => {
                    report.sent += 1;
                    for c in r
                        .body
                        .get("conflicts")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        let mut c = c.clone();
                        c["kind"] = json!(kind);
                        c["id"] = json!(id);
                        report.conflicts.push(c);
                    }
                    if let Some(key) = sync_key(&kind)
                        && record_id(&r.body).as_deref() == Some(id.as_str())
                    {
                        self.upsert(key, &r.body).await;
                    }
                }
                // Not now: signed out, server trouble or too busy. Try again later.
                401 | 408 | 429 | 500..=599 => break,
                status => {
                    // Already gone is what a delete wanted.
                    if !(method == "DELETE" && status == 404) {
                        let message = r
                            .body
                            .get("detail")
                            .or_else(|| r.body.get("title"))
                            .cloned()
                            .unwrap_or(Value::Null);
                        report.dropped.push(json!({"kind": kind, "id": id, "method": method, "status": status, "message": message}));
                    }
                }
            }
            self.done(seq).await;
        }
        report.pending = self.pending().await;
        if report.sent > 0 || !report.dropped.is_empty() {
            (self.on_report)(&report);
        }
        report
    }

    /// An outbox entry is finished: records nothing else is waiting for show the server's version again.
    async fn done(&self, seq: i64) {
        let _ = sqlx::query("DELETE FROM outbox WHERE seq = ?")
            .bind(seq)
            .execute(&self.db)
            .await;
        let _ = sqlx::query("DELETE FROM outbox_touch WHERE seq = ?")
            .bind(seq)
            .execute(&self.db)
            .await;
        let _ = sqlx::query("DELETE FROM overlay WHERE NOT EXISTS (SELECT 1 FROM outbox_touch t WHERE t.key = overlay.key AND t.id = overlay.id)")
            .execute(&self.db)
            .await;
    }

    /// Send the outbox in the background if anything is waiting.
    pub fn kick(self: &Arc<Self>) {
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            if me.pending().await > 0 {
                me.replay().await;
            }
        });
    }

    /// One of the server's records as last seen.
    pub async fn record(&self, key: &str, id: &str) -> Option<Value> {
        let json: Option<String> =
            sqlx::query_scalar("SELECT json FROM records WHERE key = ?1 AND id = ?2")
                .bind(key)
                .bind(id)
                .fetch_optional(&self.db)
                .await
                .ok()
                .flatten();
        json.and_then(|j| serde_json::from_str(&j).ok())
    }

    async fn upsert(&self, key: &str, v: &Value) {
        let Some(id) = record_id(v) else { return };
        let q = if v.get("deleted_at").is_some_and(|d| !d.is_null()) {
            sqlx::query("DELETE FROM records WHERE key = ?1 AND id = ?2")
                .bind(key)
                .bind(id)
        } else {
            sqlx::query("INSERT INTO records (key, id, json) VALUES (?1, ?2, ?3) ON CONFLICT(key, id) DO UPDATE SET json = ?3")
                .bind(key)
                .bind(id)
                .bind(v.to_string())
        };
        let _ = q.execute(&self.db).await;
    }

    /// A write the server took: the replica gets its answer.
    async fn remember_answer(&self, touched: &[Touched], answer: &Value) {
        let Some(id) = record_id(answer) else { return };
        if let Some((kind, _, _)) = touched.iter().find(|(_, i, _)| *i == id)
            && let Some(key) = sync_key(kind)
        {
            self.upsert(key, answer).await;
        }
    }

    /// A `/sync` answer: a full one replaces the replica, a delta updates it.
    async fn ingest(&self, body: &Value) {
        let Some(obj) = body.as_object() else { return };
        if obj.get("full").and_then(Value::as_bool) == Some(true) {
            let _ = sqlx::query("DELETE FROM records").execute(&self.db).await;
            let keys: Vec<&String> = obj
                .iter()
                .filter(|(_, v)| v.is_array())
                .map(|(k, _)| k)
                .collect();
            self.set_meta("keys", &json!(keys).to_string()).await;
        }
        for (k, v) in obj {
            match v {
                Value::Array(items) => {
                    if ALWAYS_COMPLETE.contains(&k.as_str()) {
                        let _ = sqlx::query("DELETE FROM records WHERE key = ?")
                            .bind(k)
                            .execute(&self.db)
                            .await;
                    }
                    for item in items {
                        self.upsert(k, item).await;
                    }
                }
                _ if k == "full" => {}
                _ if k == "rev" => {
                    let old: i64 = self
                        .meta("rev")
                        .await
                        .and_then(|r| r.parse().ok())
                        .unwrap_or(0);
                    self.set_meta("rev", &v.as_i64().unwrap_or(0).max(old).to_string())
                        .await;
                }
                _ => self.set_meta(&format!("single:{k}"), &v.to_string()).await,
            }
        }
    }

    /// A live `change` event (`{rev, kind, data}`) relayed to the page: the replica follows it too.
    pub async fn ingest_event(&self, data: &str) {
        let Ok(c) = serde_json::from_str::<Value>(data) else {
            return;
        };
        let (Some(kind), Some(record)) = (c.get("kind").and_then(Value::as_str), c.get("data"))
        else {
            return;
        };
        match kind {
            "me" | "focus_timer" | "calendar_account" => {
                self.set_meta(&format!("single:{kind}"), &record.to_string())
                    .await
            }
            _ => {
                if let Some(key) = sync_key(kind) {
                    self.upsert(key, record).await;
                }
            }
        }
    }

    /// The replica as the page last saw it: the server's records with waiting changes on top.
    async fn overlay(&self) -> Vec<(String, String, Option<Value>)> {
        sqlx::query("SELECT key, id, json FROM overlay")
            .fetch_all(&self.db)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|r| {
                let json: Option<String> = r.get("json");
                (
                    r.get("key"),
                    r.get("id"),
                    json.and_then(|j| serde_json::from_str(&j).ok()),
                )
            })
            .collect()
    }

    /// Waiting changes laid over a `/sync` answer, so a sync doesn't undo them on screen.
    async fn with_overlay(&self, mut body: Value) -> Value {
        for (key, id, after) in self.overlay().await {
            let Some(list) = body.get_mut(&key).and_then(Value::as_array_mut) else {
                continue;
            };
            list.retain(|v| record_id(v).as_deref() != Some(id.as_str()));
            if let Some(v) = after {
                list.push(v);
            }
        }
        body
    }

    async fn answer_offline(&self, path: &str) -> ApiResponse {
        let me: Option<Value> = self
            .meta("single:me")
            .await
            .and_then(|m| serde_json::from_str(&m).ok());
        if path == "/today" {
            return match me.as_ref().and_then(today) {
                Some(d) => ApiResponse {
                    status: 200,
                    body: json!({"date": d}),
                    offline: true,
                },
                None => offline(),
            };
        }
        if !is_sync(path) || me.is_none() {
            return offline();
        }
        let since: i64 = path
            .split("since=")
            .nth(1)
            .and_then(|s| s.split('&').next()?.parse().ok())
            .unwrap_or(0);
        let keys: Vec<String> = self
            .meta("keys")
            .await
            .and_then(|k| serde_json::from_str(&k).ok())
            .unwrap_or_default();
        let mut out = Map::new();
        for key in &keys {
            out.insert(key.clone(), json!([]));
        }
        let full = since == 0;
        let rows = sqlx::query("SELECT key, json FROM records ORDER BY key, id")
            .fetch_all(&self.db)
            .await
            .unwrap_or_default();
        for r in rows {
            let key: String = r.get("key");
            if !full && !ALWAYS_COMPLETE.contains(&key.as_str()) {
                continue;
            }
            let json: String = r.get("json");
            if let (Some(Value::Array(list)), Ok(v)) =
                (out.get_mut(&key), serde_json::from_str(&json))
            {
                list.push(v);
            }
        }
        let singles = sqlx::query("SELECT key, value FROM meta WHERE key LIKE 'single:%'")
            .fetch_all(&self.db)
            .await
            .unwrap_or_default();
        for r in singles {
            let key: String = r.get("key");
            let value: String = r.get("value");
            out.insert(
                key["single:".len()..].to_string(),
                serde_json::from_str(&value).unwrap_or(Value::Null),
            );
        }
        let rev: i64 = self
            .meta("rev")
            .await
            .and_then(|r| r.parse().ok())
            .unwrap_or(0);
        out.insert("rev".into(), json!(rev.max(since)));
        out.insert("full".into(), json!(full));
        out.insert(
            "server_now".into(),
            json!(chrono::Utc::now().timestamp_millis()),
        );
        if let Some(d) = me.as_ref().and_then(today) {
            out.insert("today".into(), json!(d));
        }
        let body = if full {
            self.with_overlay(Value::Object(out)).await
        } else {
            Value::Object(out)
        };
        ApiResponse {
            status: 200,
            body,
            offline: true,
        }
    }
}

fn is_sync(path: &str) -> bool {
    path == "/sync" || path.starts_with("/sync?")
}

fn offline() -> ApiResponse {
    ApiResponse {
        status: 0,
        body: Value::Null,
        offline: true,
    }
}

/// The user's day now, by their timezone and day end, as the server counts it.
fn today(me: &Value) -> Option<String> {
    use streamline_domain::time::{logical_date, parse_hhmm, parse_tz};
    let tz = parse_tz(me.get("timezone")?.as_str()?)?;
    let end = parse_hhmm(me.get("day_end")?.as_str()?)?;
    Some(
        logical_date(chrono::Utc::now(), tz, end)
            .format("%Y-%m-%d")
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queueable_writes() {
        for (m, p) in [
            ("POST", "/tasks"),
            ("PATCH", "/tasks/01J"),
            ("DELETE", "/tasks/01J"),
            ("POST", "/days/2026-10-14/entries"),
            ("PATCH", "/day-entries/01J"),
            ("PUT", "/days/2026-10-14/health"),
            ("PUT", "/days/2026-10-14/record"),
            ("POST", "/metrics/01J/entries"),
            ("PATCH", "/series/01J"),
        ] {
            assert!(queueable(m, p), "{m} {p}");
        }
        for (m, p) in [
            ("POST", "/tasks/01J/split"),
            ("POST", "/workflows/01J/start"),
            ("PATCH", "/me"),
            ("POST", "/tokens"),
            ("GET", "/tasks"),
        ] {
            assert!(!queueable(m, p), "{m} {p}");
        }
    }

    /// A real server in this process, with an admin and a device token.
    async fn server() -> (String, String) {
        let dir = std::env::temp_dir().join(format!(
            "streamline-desktop-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let (app, _) = streamline::build(streamline::config::Config::for_tests(dir))
            .await
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        });
        let http = remote::client(None);
        let res = http
            .post(format!("{url}/api/v1/setup"))
            .json(&json!({"username": "anna", "password": "correct horse"}))
            .send()
            .await
            .unwrap();
        let cookie = res.headers()[reqwest::header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let t: Value = http
            .post(format!("{url}/api/v1/tokens"))
            .header(reqwest::header::COOKIE, cookie)
            .json(&json!({"name": "test"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        (url, t["token"].as_str().unwrap().to_string())
    }

    /// Another device, straight to the server.
    async fn phone(url: &str, token: &str, method: &str, path: &str, body: Value) -> Value {
        let r = remote::client(None)
            .request(method.parse().unwrap(), format!("{url}/api/v1{path}"))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap();
        r.json().await.unwrap_or(Value::Null)
    }

    fn task(id: &str, title: &str, notes: &str) -> Value {
        json!({"id": id, "title": title, "notes": notes, "status": "open", "rev": 0})
    }

    #[tokio::test]
    async fn offline_edits_wait_and_merge() {
        let (url, token) = server().await;
        let db = std::env::temp_dir().join(format!(
            "replica-{}.sqlite",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let reports = Arc::new(Mutex::new(Vec::<Report>::new()));
        let r2 = reports.clone();
        let engine = Engine::open(&db, move |r| r2.lock().unwrap().push(r.clone()))
            .await
            .unwrap();
        engine.reset_for(&url).await.unwrap();
        engine.set_credentials(Some((url.clone(), token.clone())));

        // Online: synced, and two tasks created through the app.
        let r = engine.request("GET", "/sync?since=0", None, vec![]).await;
        assert_eq!((r.status, r.offline), (200, false));
        let (a, c) = ("01JD000000000000000000000A", "01JD000000000000000000000C");
        for (id, title) in [(a, "Laundry"), (c, "Old chore")] {
            let r = engine
                .request(
                    "POST",
                    "/tasks",
                    Some(json!({"id": id, "title": title})),
                    vec![("task".into(), id.into(), Some(task(id, title, "")))],
                )
                .await;
            assert_eq!(r.status, 200, "{}", r.body);
        }

        // Offline: an edit, a new task and an edit of a task that will be deleted elsewhere wait.
        engine.set_credentials(Some(("http://127.0.0.1:1".into(), token.clone())));
        let edited = task(a, "Laundry (offline)", "40°");
        let r = engine
            .request(
                "PATCH",
                &format!("/tasks/{a}"),
                Some(json!({"title": "Laundry (offline)", "notes": "40°"})),
                vec![("task".into(), a.into(), Some(edited.clone()))],
            )
            .await;
        assert_eq!((r.status, r.offline, &r.body), (200, true, &edited));
        let b = "01JD000000000000000000000B";
        engine
            .request(
                "POST",
                "/tasks",
                Some(json!({"id": b, "title": "Buy soap"})),
                vec![("task".into(), b.into(), Some(task(b, "Buy soap", "")))],
            )
            .await;
        engine
            .request(
                "PATCH",
                &format!("/tasks/{c}"),
                Some(json!({"notes": "still?"})),
                vec![(
                    "task".into(),
                    c.into(),
                    Some(task(c, "Old chore", "still?")),
                )],
            )
            .await;
        assert_eq!(engine.pending().await, 3);
        assert_eq!(reports.lock().unwrap().last().unwrap().pending, 3);
        // Writes that can't wait need the connection, as in the browser.
        let r = engine
            .request(
                "POST",
                &format!("/tasks/{a}/split"),
                Some(json!({})),
                vec![("task".into(), a.into(), None)],
            )
            .await;
        assert_eq!((r.status, r.offline), (0, true));
        assert_eq!(engine.pending().await, 3);

        // Restarted while offline: the app opens from the replica, with the waiting changes.
        drop(engine);
        let r3 = reports.clone();
        let engine = Engine::open(&db, move |r| r3.lock().unwrap().push(r.clone()))
            .await
            .unwrap();
        engine.set_credentials(Some(("http://127.0.0.1:1".into(), token.clone())));
        assert_eq!(engine.pending().await, 3);
        let r = engine.request("GET", "/sync?since=0", None, vec![]).await;
        assert_eq!(
            (r.status, r.offline, r.body["full"].as_bool()),
            (200, true, Some(true))
        );
        let titles: Vec<&str> = r.body["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["title"].as_str().unwrap())
            .collect();
        assert!(
            titles.contains(&"Laundry (offline)") && titles.contains(&"Buy soap"),
            "{titles:?}"
        );
        assert_eq!(r.body["me"]["username"], "anna");
        assert!(r.body["today"].is_string());
        // A delta while offline has nothing new, but the lists that always come whole.
        let r = engine.request("GET", "/sync?since=99", None, vec![]).await;
        assert_eq!(
            (
                r.body["full"].as_bool(),
                r.body["tasks"].as_array().map(Vec::len)
            ),
            (Some(false), Some(0))
        );
        assert!(r.body["groups"].is_array() && r.body["occasion_templates"].is_array());
        let r = engine.request("GET", "/today", None, vec![]).await;
        assert!(r.body["date"].is_string());
        let r = engine
            .request("GET", "/days/2026-10-14/log", None, vec![])
            .await;
        assert_eq!(r.status, 0);

        // Meanwhile on the phone: the title changed (later than the offline edit), the other task deleted.
        tokio::time::sleep(Duration::from_millis(20)).await;
        phone(
            &url,
            &token,
            "PATCH",
            &format!("/tasks/{a}"),
            json!({"title": "Laundry (phone)"}),
        )
        .await;
        phone(&url, &token, "DELETE", &format!("/tasks/{c}"), Value::Null).await;

        // Back online: sent in order and merged; the newer title stays, the deleted task's edit is dropped.
        engine.set_credentials(Some((url.clone(), token.clone())));
        let report = engine.replay().await;
        assert_eq!(
            (report.sent, report.pending, report.dropped.len()),
            (2, 0, 1),
            "{report:?}"
        );
        assert_eq!(report.dropped[0]["id"], c);
        assert_eq!(report.dropped[0]["status"], 404);
        assert_eq!(report.conflicts.len(), 1, "{report:?}");
        assert_eq!(
            (
                report.conflicts[0]["field"].as_str(),
                report.conflicts[0]["kept"].as_str()
            ),
            (Some("title"), Some("server"))
        );
        assert_eq!(report.conflicts[0]["id"], a);
        let t = phone(&url, &token, "GET", &format!("/tasks/{a}"), Value::Null).await;
        assert_eq!(
            (t["title"].as_str(), t["notes"].as_str()),
            (Some("Laundry (phone)"), Some("40°"))
        );
        assert_eq!(
            phone(&url, &token, "GET", &format!("/tasks/{b}"), Value::Null).await["title"],
            "Buy soap"
        );
        let last = reports.lock().unwrap().last().unwrap().clone();
        assert_eq!((last.sent, last.pending), (2, 0));

        // The replica shows the server's version again, and follows live events.
        let r = engine.request("GET", "/sync?since=0", None, vec![]).await;
        let titles: Vec<&str> = r.body["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["title"].as_str().unwrap())
            .collect();
        assert!(
            titles.contains(&"Laundry (phone)") && !titles.contains(&"Old chore"),
            "{titles:?}"
        );
        engine
            .ingest_event(
                &json!({"rev": 999, "kind": "task", "data": task(b, "Buy soap (2)", "")})
                    .to_string(),
            )
            .await;
        engine.set_credentials(Some(("http://127.0.0.1:1".into(), token.clone())));
        let r = engine.request("GET", "/sync?since=0", None, vec![]).await;
        assert!(
            r.body["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["title"] == "Buy soap (2)")
        );
    }

    #[tokio::test]
    async fn writes_wait_behind_earlier_ones() {
        let (url, token) = server().await;
        let db = std::env::temp_dir().join(format!(
            "replica-{}.sqlite",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let engine = Engine::open(&db, |_| {}).await.unwrap();
        engine.reset_for(&url).await.unwrap();
        engine.set_credentials(Some(("http://127.0.0.1:1".into(), token.clone())));
        let a = "01JD000000000000000000000A";
        engine
            .request(
                "POST",
                "/tasks",
                Some(json!({"id": a, "title": "One"})),
                vec![("task".into(), a.into(), Some(task(a, "One", "")))],
            )
            .await;
        // Online again but the create hasn't gone yet: the edit queues behind it, and both arrive in order.
        engine.set_credentials(Some((url.clone(), token.clone())));
        let r = engine
            .request(
                "PATCH",
                &format!("/tasks/{a}"),
                Some(json!({"title": "Two"})),
                vec![("task".into(), a.into(), Some(task(a, "Two", "")))],
            )
            .await;
        assert_eq!((r.status, r.offline), (200, false));
        for _ in 0..50 {
            if engine.pending().await == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(engine.pending().await, 0);
        assert_eq!(
            phone(&url, &token, "GET", &format!("/tasks/{a}"), Value::Null).await["title"],
            "Two"
        );
        // Signing in to another server drops the replica and anything waiting.
        engine.set_credentials(Some(("http://127.0.0.1:1".into(), token.clone())));
        engine
            .request(
                "PATCH",
                &format!("/tasks/{a}"),
                Some(json!({"title": "Three"})),
                vec![("task".into(), a.into(), Some(task(a, "Three", "")))],
            )
            .await;
        engine.reset_for("http://elsewhere:3000").await.unwrap();
        assert_eq!(engine.pending().await, 0);
    }
}
