//! Daily summary, reflection and tracking (SPEC §6.2b, Phase 5b). Everything here is
//! personal: queries filter on the user, and changes go to the user only.

use chrono::{NaiveDate, NaiveTime, Timelike};
use serde::Serialize;
use sqlx::SqliteConnection;
use streamline_domain::{
    order::key_after,
    time::{day_bounds, parse_hhmm, parse_tz},
    tracking::{Aggregate, Entry, completion_rate, daily},
};
use ts_rs::TS;

use crate::{
    AppState,
    db::next_rev,
    events::Change,
    models::{
        CalendarEvent, DayRecord, MetricDefinition, MetricEntry, Series, User, upsert_metric,
    },
    notify::{self, Notification},
    rollover::today_for,
    util::{new_id, now},
};

/// Mood and weight exist for everyone; created on first use.
pub async fn ensure_builtins(
    conn: &mut SqliteConnection,
    user_id: &str,
    changes: &mut Vec<Change>,
) -> anyhow::Result<()> {
    for (key, name, kind, unit, min, max, agg) in [
        ("mood", "Mood", "scale", "", Some(1), Some(5), "average"),
        ("weight", "Weight", "number", "kg", None, None, "latest"),
    ] {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM metric_definitions WHERE owner_user_id = ? AND key = ?)",
        )
        .bind(user_id)
        .bind(key)
        .fetch_one(&mut *conn)
        .await?;
        if exists {
            continue;
        }
        let last: Option<String> =
            sqlx::query_scalar("SELECT MAX(position) FROM metric_definitions WHERE owner_user_id = ? AND deleted_at IS NULL")
                .bind(user_id)
                .fetch_one(&mut *conn)
                .await?;
        let ts = now();
        let m = MetricDefinition {
            id: new_id(),
            owner_user_id: user_id.into(),
            key: Some(key.into()),
            name: name.into(),
            kind: kind.into(),
            unit: unit.into(),
            scale_min: min,
            scale_max: max,
            aggregate: agg.into(),
            reminder_time: None,
            last_reminded: None,
            archived: false,
            position: key_after(last.as_deref()),
            created_at: ts.clone(),
            updated_at: ts,
            deleted_at: None,
            rev: next_rev(conn).await?,
        };
        upsert_metric(conn, &m).await?;
        changes.push(Change::metric(&m));
    }
    Ok(())
}

/// A metric's value on a day (its entries combined).
#[derive(Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct MetricDay {
    pub metric_id: String,
    pub value: f64,
    /// How many entries were combined.
    pub entries: u32,
}

#[derive(Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct RoutineDay {
    pub series_id: String,
    pub title: String,
    pub done: bool,
    /// Current streak (as of now).
    pub streak: u32,
}

/// Everything about one day (SPEC §6.2b).
#[derive(Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct DaySummary {
    pub date: String,
    /// Planned tasks by outcome. `carried` moved on to a later day unfinished.
    pub planned: u32,
    pub done: u32,
    pub missed: u32,
    pub skipped: u32,
    pub carried: u32,
    pub open: u32,
    /// done / (done + missed + open); `null` when nothing counts.
    pub completion_rate: Option<f64>,
    /// Everything you completed that day, planned or not.
    pub completed_total: u32,
    /// Workflow steps completed that day.
    pub steps_done: u32,
    pub routines: Vec<RoutineDay>,
    /// Calendar events of the day (titles and times are in the event list).
    pub events: Vec<CalendarEvent>,
    pub focus_min: u32,
    /// For tasks done that day that had an estimate: estimated vs. recorded minutes.
    pub estimated_min: u32,
    pub actual_min: u32,
    pub metrics: Vec<MetricDay>,
    pub record: Option<DayRecord>,
}

fn ts(t: chrono::DateTime<chrono::Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub async fn summary(state: &AppState, user: &User, day: NaiveDate) -> anyhow::Result<DaySummary> {
    let mut conn = state.db.read.acquire().await?;
    let date = day.format("%Y-%m-%d").to_string();
    let tz = parse_tz(&user.timezone).unwrap_or(chrono_tz::UTC);
    let day_end = parse_hhmm(&user.day_end).unwrap_or(NaiveTime::MIN);
    let (from, to) = day_bounds(day, tz, day_end);

    let statuses: Vec<String> = sqlx::query_scalar(
        "SELECT t.status FROM day_entries e JOIN tasks t ON t.id = e.task_id
         WHERE e.user_id = ? AND e.date = ? AND e.deleted_at IS NULL AND t.deleted_at IS NULL",
    )
    .bind(&user.id)
    .bind(&date)
    .fetch_all(&mut *conn)
    .await?;
    let count = |s: &str| statuses.iter().filter(|x| x.as_str() == s).count() as u32;
    let (done, missed, open) = (count("done"), count("missed"), count("open"));
    let skipped = count("skipped") + count("wont_do");
    let carried: u32 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT ev.task_id) FROM task_events ev JOIN tasks t ON t.id = ev.task_id
         WHERE ev.kind = 'carried' AND json_extract(ev.data, '$.from') = ?1
           AND (t.owner_user_id = ?2 OR EXISTS (SELECT 1 FROM day_entries e WHERE e.task_id = t.id AND e.user_id = ?2))",
    )
    .bind(&date)
    .bind(&user.id)
    .fetch_one(&mut *conn)
    .await?;

    let completed_total: u32 =
        sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE completed_by = ?1 AND status = 'done' AND completed_at >= ?2 AND completed_at < ?3 AND deleted_at IS NULL")
            .bind(&user.id)
            .bind(ts(from))
            .bind(ts(to))
            .fetch_one(&mut *conn)
            .await?;
    let steps_done: u32 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tasks WHERE completed_by = ?1 AND status = 'done' AND workflow_instance_id IS NOT NULL
           AND completed_at >= ?2 AND completed_at < ?3 AND deleted_at IS NULL",
    )
    .bind(&user.id)
    .bind(ts(from))
    .bind(ts(to))
    .fetch_one(&mut *conn)
    .await?;
    let (estimated_min, actual_min): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(estimate_min), 0), COALESCE(SUM(actual_min), 0) FROM tasks
         WHERE completed_by = ?1 AND status = 'done' AND estimate_min IS NOT NULL AND completed_at >= ?2 AND completed_at < ?3 AND deleted_at IS NULL",
    )
    .bind(&user.id)
    .bind(ts(from))
    .bind(ts(to))
    .fetch_one(&mut *conn)
    .await?;
    let focus_min: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(minutes), 0) FROM focus_sessions WHERE user_id = ?1 AND kind = 'work' AND deleted_at IS NULL AND started_at >= ?2 AND started_at < ?3",
    )
    .bind(&user.id)
    .bind(ts(from))
    .bind(ts(to))
    .fetch_one(&mut *conn)
    .await?;

    // Routines with an occurrence on the day.
    let occ: Vec<(String, String)> = sqlx::query_as(
        "SELECT t.series_id, t.status FROM tasks t
         WHERE (t.owner_user_id = ?1 OR t.owner_group_id IN (SELECT group_id FROM group_members WHERE user_id = ?1))
           AND t.series_id IS NOT NULL AND t.deleted_at IS NULL AND t.occurrence_date = ?2 AND t.window_end IS NULL",
    )
    .bind(&user.id)
    .bind(&date)
    .fetch_all(&mut *conn)
    .await?;
    let mut routines = vec![];
    for (sid, status) in occ {
        let Some(s) = sqlx::query_as::<_, Series>("SELECT * FROM series WHERE id = ?")
            .bind(&sid)
            .fetch_optional(&mut *conn)
            .await?
        else {
            continue;
        };
        let streak = crate::routines::stats(state, user, &s)
            .await
            .map(|x| x.streak)
            .unwrap_or(0);
        routines.push(RoutineDay {
            series_id: sid,
            title: s.title,
            done: status == "done",
            streak,
        });
    }
    routines.sort_by(|a, b| a.title.cmp(&b.title));

    let events: Vec<CalendarEvent> = {
        let midnight =
            |d: NaiveDate| streamline_domain::time::resolve_local(tz, d.and_time(NaiveTime::MIN));
        let (f, t) = (midnight(day), midnight(day + chrono::Duration::days(1)));
        let iso =
            |x: chrono::DateTime<chrono::Utc>| x.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        sqlx::query_as(
            "SELECT e.* FROM events e JOIN calendars c ON c.id = e.calendar_id
             WHERE e.user_id = ?1 AND e.deleted_at IS NULL AND c.enabled = 1 AND (
               e.all_day = 0 AND e.start_at < ?3 AND e.end_at > ?2 OR e.all_day = 1 AND e.start_date <= ?4 AND e.end_date > ?4)
             ORDER BY e.all_day DESC, e.start_at",
        )
        .bind(&user.id)
        .bind(iso(f))
        .bind(iso(t))
        .bind(&date)
        .fetch_all(&mut *conn)
        .await?
    };

    let defs: Vec<MetricDefinition> =
        sqlx::query_as("SELECT * FROM metric_definitions WHERE owner_user_id = ? AND deleted_at IS NULL ORDER BY position")
            .bind(&user.id)
            .fetch_all(&mut *conn)
            .await?;
    let mut metrics = vec![];
    for d in &defs {
        let es: Vec<MetricEntry> = sqlx::query_as(
            "SELECT * FROM metric_entries WHERE metric_id = ? AND date = ? AND deleted_at IS NULL",
        )
        .bind(&d.id)
        .bind(&date)
        .fetch_all(&mut *conn)
        .await?;
        if es.is_empty() {
            continue;
        }
        let entries: Vec<Entry> = es
            .iter()
            .map(|e| Entry {
                date: day,
                at: e.at.clone(),
                value: e.value,
            })
            .collect();
        let agg = Aggregate::parse(&d.aggregate).unwrap_or(Aggregate::Latest);
        if let Some(v) = daily(&entries, agg).get(&day) {
            metrics.push(MetricDay {
                metric_id: d.id.clone(),
                value: *v,
                entries: es.len() as u32,
            });
        }
    }
    let record = sqlx::query_as(
        "SELECT * FROM day_records WHERE user_id = ? AND date = ? AND deleted_at IS NULL",
    )
    .bind(&user.id)
    .bind(&date)
    .fetch_optional(&mut *conn)
    .await?;

    Ok(DaySummary {
        date,
        planned: statuses.len() as u32 + carried,
        done,
        missed,
        skipped,
        carried,
        open,
        completion_rate: completion_rate(done, missed, open),
        completed_total,
        steps_done,
        routines,
        events,
        focus_min: focus_min.max(0) as u32,
        estimated_min: estimated_min.max(0) as u32,
        actual_min: actual_min.max(0) as u32,
        metrics,
        record,
    })
}

/// One day at a glance, for the month calendar.
#[derive(Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct DayGlance {
    pub date: String,
    pub planned: u32,
    pub done: u32,
    /// The day's mood (average), if logged.
    pub mood: Option<f64>,
    pub journal: bool,
}

pub async fn glance(
    state: &AppState,
    user: &User,
    from: NaiveDate,
    to: NaiveDate,
) -> anyhow::Result<Vec<DayGlance>> {
    let mut conn = state.db.read.acquire().await?;
    let (f, t) = (
        from.format("%Y-%m-%d").to_string(),
        to.format("%Y-%m-%d").to_string(),
    );
    let plan: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT e.date, COUNT(*), SUM(t.status = 'done') FROM day_entries e JOIN tasks t ON t.id = e.task_id
         WHERE e.user_id = ?1 AND e.deleted_at IS NULL AND t.deleted_at IS NULL AND e.date >= ?2 AND e.date <= ?3 GROUP BY e.date",
    )
    .bind(&user.id)
    .bind(&f)
    .bind(&t)
    .fetch_all(&mut *conn)
    .await?;
    let moods: Vec<(String, f64)> = sqlx::query_as(
        "SELECT me.date, AVG(me.value) FROM metric_entries me JOIN metric_definitions md ON md.id = me.metric_id
         WHERE me.user_id = ?1 AND md.key = 'mood' AND me.deleted_at IS NULL AND me.date >= ?2 AND me.date <= ?3 GROUP BY me.date",
    )
    .bind(&user.id)
    .bind(&f)
    .bind(&t)
    .fetch_all(&mut *conn)
    .await?;
    let journals: Vec<String> = sqlx::query_scalar(
        "SELECT date FROM day_records WHERE user_id = ?1 AND deleted_at IS NULL AND date >= ?2 AND date <= ?3
           AND (journal <> '' OR went_well <> '' OR went_badly <> '' OR tomorrow <> '')",
    )
    .bind(&user.id)
    .bind(&f)
    .bind(&t)
    .fetch_all(&mut *conn)
    .await?;
    let mut out = vec![];
    let mut d = from;
    while d <= to {
        let ds = d.format("%Y-%m-%d").to_string();
        let p = plan.iter().find(|x| x.0 == ds);
        out.push(DayGlance {
            planned: p.map_or(0, |x| x.1 as u32),
            done: p.map_or(0, |x| x.2 as u32),
            mood: moods.iter().find(|x| x.0 == ds).map(|x| x.1),
            journal: journals.contains(&ds),
            date: ds,
        });
        d = d.succ_opt().unwrap();
    }
    Ok(out)
}

/// Background: remind about metrics not logged by their reminder time (once a day).
pub async fn remind_all(state: &AppState) -> anyhow::Result<()> {
    let due: Vec<MetricDefinition> = sqlx::query_as(
        "SELECT * FROM metric_definitions WHERE deleted_at IS NULL AND archived = 0 AND reminder_time IS NOT NULL",
    )
    .fetch_all(&state.db.read)
    .await?;
    for mut m in due {
        let Some(user) =
            sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ? AND deleted_at IS NULL")
                .bind(&m.owner_user_id)
                .fetch_optional(&state.db.read)
                .await?
        else {
            continue;
        };
        let today = today_for(&user).format("%Y-%m-%d").to_string();
        if m.last_reminded.as_deref() == Some(&today) {
            continue;
        }
        let tz = parse_tz(&user.timezone).unwrap_or(chrono_tz::UTC);
        let now_local = chrono::Utc::now().with_timezone(&tz).time();
        let at = m
            .reminder_time
            .as_deref()
            .and_then(parse_hhmm)
            .unwrap_or(NaiveTime::MIN);
        let day_end = parse_hhmm(&user.day_end).unwrap_or(NaiveTime::MIN);
        // Within the logical day: minutes since the day started.
        let rel = |t: NaiveTime| {
            ((t.hour() * 60 + t.minute()) + 1440 - (day_end.hour() * 60 + day_end.minute())) % 1440
        };
        if rel(now_local) < rel(at) {
            continue;
        }
        let logged: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM metric_entries WHERE metric_id = ? AND date = ? AND deleted_at IS NULL)")
            .bind(&m.id)
            .bind(&today)
            .fetch_one(&state.db.read)
            .await?;
        let mut tx = state.db.write.begin().await?;
        m.last_reminded = Some(today);
        sqlx::query("UPDATE metric_definitions SET last_reminded = ? WHERE id = ?")
            .bind(&m.last_reminded)
            .bind(&m.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        if !logged {
            notify::send(
                state,
                &user.id,
                &Notification {
                    kind: "metric".into(),
                    title: format!("Log your {}", m.name.to_lowercase()),
                    body: "Nothing logged yet today.".into(),
                    url: "/".into(),
                },
            );
        }
    }
    Ok(())
}
