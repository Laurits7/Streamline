//! Reflection, metrics, the day summary and data export (SPEC §6.2b). All personal.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use streamline_domain::{order::key_after, tracking::parse_csv};

use crate::{
    AppState,
    auth::AuthUser,
    db::next_rev,
    error::{ApiResult, AppError, bad},
    events::Change,
    models::{
        DayRecord, MetricDefinition, MetricEntry, upsert_day_record, upsert_metric,
        upsert_metric_entry,
    },
    tracking::{self, DayGlance, DaySummary},
    util::{check_hhmm, double_option, id_or_new, new_id, now, parse_date},
};

// ----- Reflection -----

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PutRecord {
    journal: Option<String>,
    went_well: Option<String>,
    went_badly: Option<String>,
    tomorrow: Option<String>,
}

#[utoipa::path(put, path = "/days/{date}/record", tag = "tracking", summary = "Save the day's reflection (fields left out are kept)", params(("date" = String, Path, description = "YYYY-MM-DD")), request_body = PutRecord, responses((status = 200, body = DayRecord), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn put_record(
    State(state): State<AppState>,
    user: AuthUser,
    Path(date): Path<String>,
    Json(c): Json<PutRecord>,
) -> ApiResult<Json<DayRecord>> {
    parse_date(&date)?;
    for f in [&c.journal, &c.went_well, &c.went_badly, &c.tomorrow]
        .into_iter()
        .flatten()
    {
        if f.len() > 100_000 {
            return Err(bad("text too long"));
        }
    }
    let mut tx = state.db.write.begin().await?;
    let ts = now();
    let mut r: DayRecord =
        match sqlx::query_as("SELECT * FROM day_records WHERE user_id = ? AND date = ?")
            .bind(user.id())
            .bind(&date)
            .fetch_optional(&mut *tx)
            .await?
        {
            Some(r) => r,
            None => DayRecord {
                id: new_id(),
                user_id: user.id().into(),
                date,
                journal: String::new(),
                went_well: String::new(),
                went_badly: String::new(),
                tomorrow: String::new(),
                created_at: ts.clone(),
                updated_at: ts.clone(),
                deleted_at: None,
                rev: 0,
            },
        };
    if let Some(v) = c.journal {
        r.journal = v;
    }
    if let Some(v) = c.went_well {
        r.went_well = v;
    }
    if let Some(v) = c.went_badly {
        r.went_badly = v;
    }
    if let Some(v) = c.tomorrow {
        r.tomorrow = v;
    }
    r.deleted_at = None;
    r.updated_at = ts;
    r.rev = next_rev(&mut tx).await?;
    upsert_day_record(&mut tx, &r).await?;
    tx.commit().await?;
    state.bus.publish([Change::day_record(&r)]);
    Ok(Json(r))
}

#[utoipa::path(get, path = "/days/{date}/summary", tag = "tracking", summary = "The day's summary: planned vs. done, routines, events, focus, metrics and reflection", params(("date" = String, Path, description = "YYYY-MM-DD")), responses((status = 200, body = DaySummary), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn summary(
    State(state): State<AppState>,
    user: AuthUser,
    Path(date): Path<String>,
) -> ApiResult<Json<DaySummary>> {
    let day = parse_date(&date)?;
    Ok(Json(tracking::summary(&state, &user.user, day).await?))
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RangeQuery {
    from: String,
    to: String,
}

#[utoipa::path(get, path = "/summaries", tag = "tracking", summary = "Days at a glance (for a month calendar), at most 62 days", params(RangeQuery), responses((status = 200, body = Vec<DayGlance>), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn glance(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<RangeQuery>,
) -> ApiResult<Json<Vec<DayGlance>>> {
    let (from, to) = (parse_date(&q.from)?, parse_date(&q.to)?);
    if to < from || (to - from).num_days() > 62 {
        return Err(bad("the range must be 0-62 days"));
    }
    Ok(Json(tracking::glance(&state, &user.user, from, to).await?))
}

// ----- Metric definitions -----

const KINDS: [&str; 3] = ["number", "scale", "yes_no"];
const AGGREGATES: [&str; 4] = ["latest", "average", "sum", "max"];

fn check_metric(m: &MetricDefinition) -> ApiResult<()> {
    if m.name.trim().is_empty() || m.name.chars().count() > 60 {
        return Err(bad("name must be 1-60 characters"));
    }
    if !KINDS.contains(&m.kind.as_str()) {
        return Err(bad("kind must be number, scale or yes_no"));
    }
    if !AGGREGATES.contains(&m.aggregate.as_str()) {
        return Err(bad("aggregate must be latest, average, sum or max"));
    }
    if m.unit.chars().count() > 20 {
        return Err(bad("unit is too long"));
    }
    if m.kind == "scale" {
        match (m.scale_min, m.scale_max) {
            (Some(a), Some(b)) if a < b && b - a <= 20 => {}
            _ => {
                return Err(bad(
                    "a scale needs a minimum below its maximum (at most 20 steps)",
                ));
            }
        }
    }
    if let Some(t) = &m.reminder_time {
        check_hhmm(t)?;
    }
    Ok(())
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateMetric {
    id: Option<String>,
    name: String,
    /// `number`, `scale` or `yes_no`.
    kind: String,
    #[serde(default)]
    unit: String,
    scale_min: Option<i32>,
    scale_max: Option<i32>,
    /// Defaults: number → latest, scale → average, yes_no → max.
    aggregate: Option<String>,
    reminder_time: Option<String>,
}

#[utoipa::path(post, path = "/metrics", tag = "tracking", summary = "Track something new (sleep, water, steps…)", request_body = CreateMetric, responses((status = 200, body = MetricDefinition), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn create_metric(
    State(state): State<AppState>,
    user: AuthUser,
    Json(c): Json<CreateMetric>,
) -> ApiResult<Json<MetricDefinition>> {
    let id = id_or_new(c.id)?;
    let mut tx = state.db.write.begin().await?;
    if let Some(m) =
        sqlx::query_as::<_, MetricDefinition>("SELECT * FROM metric_definitions WHERE id = ?")
            .bind(&id)
            .fetch_optional(&mut *tx)
            .await?
    {
        return if m.owner_user_id == user.id() {
            Ok(Json(m))
        } else {
            Err(AppError::Conflict("id already in use".into()))
        };
    }
    let last: Option<String> = sqlx::query_scalar("SELECT MAX(position) FROM metric_definitions WHERE owner_user_id = ? AND deleted_at IS NULL")
        .bind(user.id())
        .fetch_one(&mut *tx)
        .await?;
    let ts = now();
    let aggregate = c.aggregate.unwrap_or_else(|| match c.kind.as_str() {
        "scale" => "average".into(),
        "yes_no" => "max".into(),
        _ => "latest".into(),
    });
    let (scale_min, scale_max) = if c.kind == "scale" {
        (c.scale_min.or(Some(1)), c.scale_max.or(Some(5)))
    } else {
        (None, None)
    };
    let m = MetricDefinition {
        id,
        owner_user_id: user.id().into(),
        key: None,
        name: c.name.trim().into(),
        kind: c.kind,
        unit: c.unit.trim().into(),
        scale_min,
        scale_max,
        aggregate,
        reminder_time: c.reminder_time,
        last_reminded: None,
        archived: false,
        position: key_after(last.as_deref()),
        created_at: ts.clone(),
        updated_at: ts,
        deleted_at: None,
        rev: next_rev(&mut tx).await?,
    };
    check_metric(&m)?;
    upsert_metric(&mut tx, &m).await?;
    tx.commit().await?;
    state.bus.publish([Change::metric(&m)]);
    Ok(Json(m))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchMetric {
    name: Option<String>,
    unit: Option<String>,
    scale_min: Option<i32>,
    scale_max: Option<i32>,
    aggregate: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    reminder_time: Option<Option<String>>,
    archived: Option<bool>,
    position: Option<String>,
}

async fn load_metric(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    id: &str,
) -> ApiResult<MetricDefinition> {
    sqlx::query_as("SELECT * FROM metric_definitions WHERE id = ? AND owner_user_id = ? AND deleted_at IS NULL")
        .bind(id)
        .bind(user.id())
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound)
}

#[utoipa::path(patch, path = "/metrics/{id}", tag = "tracking", summary = "Rename, archive or set a reminder for a metric", params(("id" = String, Path, description = "ULID")), request_body = PatchMetric, responses((status = 200, body = MetricDefinition), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn patch_metric(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(c): Json<PatchMetric>,
) -> ApiResult<Json<MetricDefinition>> {
    let mut tx = state.db.write.begin().await?;
    let mut m = load_metric(&mut tx, &user, &id).await?;
    if let Some(v) = c.name {
        m.name = v.trim().into();
    }
    // The built-in weight is always stored in kg (shown in lb for imperial units).
    if let Some(v) = c.unit.filter(|_| m.key.as_deref() != Some("weight")) {
        m.unit = v.trim().into();
    }
    if m.kind == "scale" && m.key.is_none() {
        if let Some(v) = c.scale_min {
            m.scale_min = Some(v);
        }
        if let Some(v) = c.scale_max {
            m.scale_max = Some(v);
        }
    }
    if let Some(v) = c.aggregate {
        m.aggregate = v;
    }
    if let Some(v) = c.reminder_time {
        m.reminder_time = v;
        m.last_reminded = None;
    }
    if let Some(v) = c.archived {
        m.archived = v;
    }
    if let Some(v) = c.position {
        crate::util::check_position(&v)?;
        m.position = v;
    }
    check_metric(&m)?;
    m.updated_at = now();
    m.rev = next_rev(&mut tx).await?;
    upsert_metric(&mut tx, &m).await?;
    tx.commit().await?;
    state.bus.publish([Change::metric(&m)]);
    Ok(Json(m))
}

/// Delete a custom metric and its entries (built-ins can only be archived).
#[utoipa::path(delete, path = "/metrics/{id}", tag = "tracking", summary = "Delete a custom metric and everything logged for it", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 400, description = "Built-in metrics can only be archived", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete_metric(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.write.begin().await?;
    let mut m = load_metric(&mut tx, &user, &id).await?;
    if m.key.is_some() {
        return Err(bad(
            "built-in metrics can't be deleted; archive them instead",
        ));
    }
    let ts = now();
    let mut changes = vec![];
    let entries: Vec<MetricEntry> =
        sqlx::query_as("SELECT * FROM metric_entries WHERE metric_id = ? AND deleted_at IS NULL")
            .bind(&m.id)
            .fetch_all(&mut *tx)
            .await?;
    for mut e in entries {
        e.deleted_at = Some(ts.clone());
        e.updated_at = ts.clone();
        e.rev = next_rev(&mut tx).await?;
        upsert_metric_entry(&mut tx, &e).await?;
        changes.push(Change::metric_entry(&e));
    }
    m.deleted_at = Some(ts.clone());
    m.updated_at = ts;
    m.rev = next_rev(&mut tx).await?;
    upsert_metric(&mut tx, &m).await?;
    changes.push(Change::metric(&m));
    tx.commit().await?;
    state.bus.publish(changes);
    Ok(StatusCode::NO_CONTENT)
}

// ----- Entries -----

fn check_value(m: &MetricDefinition, v: f64) -> ApiResult<()> {
    if !v.is_finite() || v.abs() > 1e9 {
        return Err(bad("value out of range"));
    }
    match m.kind.as_str() {
        "scale"
            if !(f64::from(m.scale_min.unwrap_or(1))..=f64::from(m.scale_max.unwrap_or(5)))
                .contains(&v) =>
        {
            Err(bad("value is outside the scale"))
        }
        "yes_no" if v != 0.0 && v != 1.0 => Err(bad("yes/no values are 1 or 0")),
        _ => Ok(()),
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateEntry {
    id: Option<String>,
    /// Defaults to today (your logical day).
    date: Option<String>,
    value: f64,
    #[serde(default)]
    note: String,
}

#[utoipa::path(post, path = "/metrics/{id}/entries", tag = "tracking", summary = "Log a value (weight in kg)", params(("id" = String, Path, description = "Metric ULID")), request_body = CreateEntry, responses((status = 200, body = MetricEntry), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn create_entry(
    State(state): State<AppState>,
    user: AuthUser,
    Path(metric_id): Path<String>,
    Json(c): Json<CreateEntry>,
) -> ApiResult<Json<MetricEntry>> {
    let id = id_or_new(c.id)?;
    let mut tx = state.db.write.begin().await?;
    let m = load_metric(&mut tx, &user, &metric_id).await?;
    check_value(&m, c.value)?;
    if let Some(e) = sqlx::query_as::<_, MetricEntry>("SELECT * FROM metric_entries WHERE id = ?")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?
    {
        return if e.user_id == user.id() {
            Ok(Json(e))
        } else {
            Err(AppError::Conflict("id already in use".into()))
        };
    }
    let date = match c.date {
        Some(d) => {
            parse_date(&d)?;
            d
        }
        None => crate::rollover::today_for(&user.user)
            .format("%Y-%m-%d")
            .to_string(),
    };
    if c.note.len() > 2000 {
        return Err(bad("note too long"));
    }
    let ts = now();
    let e = MetricEntry {
        id,
        user_id: user.id().into(),
        metric_id,
        date,
        at: ts.clone(),
        value: c.value,
        note: c.note,
        created_at: ts.clone(),
        updated_at: ts,
        deleted_at: None,
        rev: next_rev(&mut tx).await?,
    };
    upsert_metric_entry(&mut tx, &e).await?;
    tx.commit().await?;
    state.bus.publish([Change::metric_entry(&e)]);
    Ok(Json(e))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchEntry {
    value: Option<f64>,
    note: Option<String>,
    date: Option<String>,
}

async fn load_entry(
    conn: &mut sqlx::SqliteConnection,
    user: &AuthUser,
    id: &str,
) -> ApiResult<MetricEntry> {
    sqlx::query_as(
        "SELECT * FROM metric_entries WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user.id())
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound)
}

#[utoipa::path(patch, path = "/metric-entries/{id}", tag = "tracking", summary = "Correct a logged value", params(("id" = String, Path, description = "ULID")), request_body = PatchEntry, responses((status = 200, body = MetricEntry), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn patch_entry(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(c): Json<PatchEntry>,
) -> ApiResult<Json<MetricEntry>> {
    let mut tx = state.db.write.begin().await?;
    let mut e = load_entry(&mut tx, &user, &id).await?;
    let m = load_metric(&mut tx, &user, &e.metric_id).await?;
    if let Some(v) = c.value {
        check_value(&m, v)?;
        e.value = v;
    }
    if let Some(v) = c.note {
        e.note = v;
    }
    if let Some(d) = c.date {
        parse_date(&d)?;
        e.date = d;
    }
    e.updated_at = now();
    e.rev = next_rev(&mut tx).await?;
    upsert_metric_entry(&mut tx, &e).await?;
    tx.commit().await?;
    state.bus.publish([Change::metric_entry(&e)]);
    Ok(Json(e))
}

#[utoipa::path(delete, path = "/metric-entries/{id}", tag = "tracking", summary = "Delete a logged value", params(("id" = String, Path, description = "ULID")), responses((status = 204, description = "Done"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn delete_entry(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.db.write.begin().await?;
    let mut e = load_entry(&mut tx, &user, &id).await?;
    e.deleted_at = Some(now());
    e.updated_at = now();
    e.rev = next_rev(&mut tx).await?;
    upsert_metric_entry(&mut tx, &e).await?;
    tx.commit().await?;
    state.bus.publish([Change::metric_entry(&e)]);
    Ok(StatusCode::NO_CONTENT)
}

// ----- Export and import -----

#[utoipa::path(get, path = "/metrics/{id}/csv", tag = "tracking", summary = "A metric's entries as CSV (date, value, note, logged_at)", params(("id" = String, Path, description = "ULID")), responses((status = 200, body = String, content_type = "text/csv"), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn metric_csv(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let mut conn = state.db.read.acquire().await?;
    let m = load_metric(&mut conn, &user, &id).await?;
    let entries: Vec<MetricEntry> = sqlx::query_as(
        "SELECT * FROM metric_entries WHERE metric_id = ? AND deleted_at IS NULL ORDER BY date, at",
    )
    .bind(&m.id)
    .fetch_all(&mut *conn)
    .await?;
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut out = String::from("date,value,note,logged_at\n");
    for e in entries {
        out.push_str(&format!(
            "{},{},{},{}\n",
            e.date,
            e.value,
            quote(&e.note),
            e.at
        ));
    }
    let file = format!(
        "attachment; filename=\"{}.csv\"",
        m.name
            .replace(|c: char| !c.is_alphanumeric(), "-")
            .to_lowercase()
    );
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
            (header::CONTENT_DISPOSITION, file),
        ],
        out,
    ))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ImportCsv {
    /// Rows of `date,value[,note]` (header optional; `;` or tab also work).
    text: String,
}

#[derive(Serialize, utoipa::ToSchema, ts_rs::TS)]
#[ts(export)]
pub struct ImportResult {
    pub imported: u32,
}

#[utoipa::path(post, path = "/metrics/{id}/import", tag = "tracking", summary = "Import past values from CSV (weight in kg)", params(("id" = String, Path, description = "ULID")), request_body = ImportCsv, responses((status = 200, body = ImportResult), (status = 400, description = "Invalid input", body = crate::error::Problem), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn import_csv(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(c): Json<ImportCsv>,
) -> ApiResult<Json<ImportResult>> {
    if c.text.len() > 5_000_000 {
        return Err(bad("file too large"));
    }
    let rows = parse_csv(&c.text).map_err(bad)?;
    let mut tx = state.db.write.begin().await?;
    let m = load_metric(&mut tx, &user, &id).await?;
    for (_, v, _) in &rows {
        check_value(&m, *v)?;
    }
    let mut changes = vec![];
    for (date, value, note) in rows.iter() {
        let at = format!("{date}T12:00:00.000Z");
        let ts = now();
        let e = MetricEntry {
            id: new_id(),
            user_id: user.id().into(),
            metric_id: m.id.clone(),
            date: date.format("%Y-%m-%d").to_string(),
            at,
            value: *value,
            note: note.chars().take(2000).collect(),
            created_at: ts.clone(),
            updated_at: ts,
            deleted_at: None,
            rev: next_rev(&mut tx).await?,
        };
        upsert_metric_entry(&mut tx, &e).await?;
        changes.push(Change::metric_entry(&e));
    }
    tx.commit().await?;
    // A big import would flood live clients: tell them to resync instead.
    if changes.len() > 200 {
        state.bus.publish([Change::membership(user.id())]);
    } else {
        state.bus.publish(changes);
    }
    Ok(Json(ImportResult {
        imported: rows.len() as u32,
    }))
}

/// Everything that is yours, as one JSON document.
#[utoipa::path(get, path = "/export", tag = "tracking", summary = "Download all your data as JSON", responses((status = 200, description = "A JSON document", body = Object), (status = 401, description = "Not signed in", body = crate::error::Problem)))]
pub async fn export(State(state): State<AppState>, user: AuthUser) -> ApiResult<impl IntoResponse> {
    use serde_json::{Value, json};
    let db = &state.db.read;
    let uid = user.id();
    async fn rows(db: &sqlx::SqlitePool, sql: &str, uid: &str) -> Result<Value, AppError> {
        // Every column as JSON, in the order SQLite returns them.
        let wrapped = format!("SELECT json_group_array(json(j)) FROM (SELECT {sql})");
        let s: Option<String> = sqlx::query_scalar(&wrapped).bind(uid).fetch_one(db).await?;
        Ok(serde_json::from_str(&s.unwrap_or_else(|| "[]".into())).unwrap_or(Value::Array(vec![])))
    }
    let obj = |table: &str, owner: &str| {
        format!("json_object(*) AS j FROM {table} WHERE {owner} = ?1 AND deleted_at IS NULL")
    };
    // SQLite has no json_object(*): build each table's object from its columns.
    let mut out = serde_json::Map::new();
    out.insert("exported_at".into(), json!(now()));
    out.insert(
        "user".into(),
        serde_json::to_value(crate::models::Me::from(&user.user)).unwrap_or_default(),
    );
    for (name, table, owner) in [
        ("projects", "projects", "owner_user_id"),
        ("tasks", "tasks", "owner_user_id"),
        ("day_entries", "day_entries", "user_id"),
        ("day_plans", "day_plans", "user_id"),
        ("day_records", "day_records", "user_id"),
        ("metrics", "metric_definitions", "owner_user_id"),
        ("metric_entries", "metric_entries", "user_id"),
        ("focus_sessions", "focus_sessions", "user_id"),
        ("routines", "series", "owner_user_id"),
        ("places", "places", "owner_user_id"),
        ("workflows", "workflow_templates", "owner_user_id"),
        ("default_tasks", "task_templates", "owner_user_id"),
        ("people", "people", "owner_user_id"),
        ("day_templates", "day_templates", "owner_user_id"),
        ("time_blocks", "time_blocks", "user_id"),
        ("event_projects", "event_projects", "user_id"),
    ] {
        let cols: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info(?)")
            .bind(table)
            .fetch_all(db)
            .await?;
        let pairs = cols
            .iter()
            .map(|c| format!("'{c}', \"{c}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = obj(table, owner).replace("json_object(*)", &format!("json_object({pairs})"));
        out.insert(name.into(), rows(db, &sql, uid).await?);
    }
    let body = serde_json::to_string_pretty(&Value::Object(out)).unwrap_or_default();
    let file = format!(
        "attachment; filename=\"streamline-{}-{}.json\"",
        user.user.username,
        &now()[..10]
    );
    Ok((
        [
            (header::CONTENT_TYPE, "application/json".to_string()),
            (header::CONTENT_DISPOSITION, file),
        ],
        body,
    ))
}
