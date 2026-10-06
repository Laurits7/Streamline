//! Time blocks, day templates and conflicts (SPEC §6.8, §6.11; Phase 5a).
//! Weekday templates are applied to the coming week's days once (`block_days` records
//! it), so later edits and deletions stick. Conflicts are computed for a local calendar
//! day from timed tasks, busy events and blocks (`domain::conflicts`).

use std::{collections::HashSet, sync::Mutex};

use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Timelike};
use sqlx::SqliteConnection;
use streamline_domain::{
    conflicts::{self, Conflict, Item, Kind},
    time::{parse_hhmm, parse_tz, resolve_local},
};

use crate::{
    AppState,
    db::next_rev,
    events::Change,
    models::{CalendarEvent, DayTemplate, Task, TimeBlock, User, upsert_time_block},
    notify::{self, Notification},
    rollover::today_for,
    util::{new_id, now},
};

pub fn minutes(hhmm: &str) -> Option<u32> {
    parse_hhmm(hhmm).map(|t| t.hour() * 60 + t.minute())
}

pub fn hhmm(m: u32) -> String {
    format!("{:02}:{:02}", m / 60 % 24, m % 60)
}

fn fmt(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// The user changed the day's blocks by hand: weekday templates leave it alone from now on.
pub async fn mark_manual(
    conn: &mut SqliteConnection,
    user_id: &str,
    date: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO block_days (user_id, date, manual) VALUES (?, ?, 1) ON CONFLICT(user_id, date) DO UPDATE SET manual = 1",
    )
    .bind(user_id)
    .bind(date)
    .execute(conn)
    .await
    .map(|_| ())
}

/// Replace the day's blocks with the template's (`None` clears them). Applying the
/// template the day already has is a no-op.
pub async fn apply(
    conn: &mut SqliteConnection,
    user_id: &str,
    date: &str,
    tpl: Option<&DayTemplate>,
) -> anyhow::Result<Vec<Change>> {
    let existing: Vec<TimeBlock> = sqlx::query_as(
        "SELECT * FROM time_blocks WHERE user_id = ? AND date = ? AND deleted_at IS NULL",
    )
    .bind(user_id)
    .bind(date)
    .fetch_all(&mut *conn)
    .await?;
    if let Some(t) = tpl
        && !existing.is_empty()
        && existing
            .iter()
            .all(|b| b.template_id.as_deref() == Some(&t.id))
    {
        return Ok(vec![]);
    }
    let mut changes = vec![];
    let ts = now();
    for mut b in existing {
        b.deleted_at = Some(ts.clone());
        b.updated_at = ts.clone();
        b.rev = next_rev(conn).await?;
        upsert_time_block(conn, &b).await?;
        changes.push(Change::time_block(&b));
    }
    for tb in tpl.map(|t| t.blocks.0.as_slice()).unwrap_or_default() {
        let b = TimeBlock {
            id: new_id(),
            user_id: user_id.into(),
            date: date.into(),
            title: tb.title.clone(),
            start_time: tb.start.clone(),
            end_time: tb.end.clone(),
            energy: tb.energy.clone(),
            template_id: tpl.map(|t| t.id.clone()),
            created_at: ts.clone(),
            updated_at: ts.clone(),
            deleted_at: None,
            rev: next_rev(conn).await?,
        };
        upsert_time_block(conn, &b).await?;
        changes.push(Change::time_block(&b));
    }
    Ok(changes)
}

/// Keep the user's next 8 days in line with their weekday templates, except days whose
/// blocks were changed by hand.
pub async fn materialize_user(state: &AppState, user: &User) -> anyhow::Result<()> {
    let templates: Vec<DayTemplate> =
        sqlx::query_as("SELECT * FROM day_templates WHERE owner_user_id = ? AND deleted_at IS NULL ORDER BY position")
            .bind(&user.id)
            .fetch_all(&state.db.read)
            .await?;
    let today = today_for(user);
    let mut tx = state.db.write.begin().await?;
    let mut changes = vec![];
    for i in 0..8 {
        let day = today + Duration::days(i);
        let date = fmt(day);
        let row: Option<(bool, Option<String>, Option<i64>)> =
            sqlx::query_as("SELECT manual, template_id, template_rev FROM block_days WHERE user_id = ? AND date = ?")
                .bind(&user.id)
                .bind(&date)
                .fetch_optional(&mut *tx)
                .await?;
        let wd = day.weekday().number_from_monday() as i32;
        let want = templates.iter().find(|t| t.weekdays.0.contains(&wd));
        match (&row, want) {
            (Some((true, _, _)), _) | (None, None) => continue,
            (Some((false, id, rev)), Some(t))
                if id.as_deref() == Some(&t.id) && *rev == Some(t.rev) =>
            {
                continue;
            }
            (Some((false, None, _)), None) => continue,
            _ => {}
        }
        changes.extend(apply(&mut tx, &user.id, &date, want).await?);
        sqlx::query(
            "INSERT INTO block_days (user_id, date, manual, template_id, template_rev) VALUES (?, ?, 0, ?, ?)
             ON CONFLICT(user_id, date) DO UPDATE SET template_id = excluded.template_id, template_rev = excluded.template_rev",
        )
        .bind(&user.id)
        .bind(&date)
        .bind(want.map(|t| t.id.clone()))
        .bind(want.map(|t| t.rev))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    state.bus.publish(changes);
    Ok(())
}

pub async fn materialize_all(state: &AppState) -> anyhow::Result<()> {
    let users: Vec<User> = sqlx::query_as(
        "SELECT * FROM users WHERE deleted_at IS NULL AND (id IN (SELECT owner_user_id FROM day_templates WHERE deleted_at IS NULL AND weekdays <> '[]')
           OR id IN (SELECT user_id FROM block_days WHERE manual = 0 AND template_id IS NOT NULL))",
    )
    .fetch_all(&state.db.read)
    .await?;
    for u in users {
        if let Err(e) = materialize_user(state, &u).await {
            tracing::warn!("day templates failed for user {}: {e:#}", u.id);
        }
    }
    Ok(())
}

/// Everything that takes time on a local calendar day, as conflict items.
pub struct DayItems {
    pub items: Vec<Item>,
}

/// Busy calendar events on the day as (id, start, end) minutes, clipped to the day.
/// All-day events count only for calendars marked "all-day events block the day".
pub async fn busy_events(
    conn: &mut SqliteConnection,
    user: &User,
    day: NaiveDate,
) -> anyhow::Result<Vec<(String, u32, u32)>> {
    let tz = parse_tz(&user.timezone).unwrap_or(chrono_tz::UTC);
    let from = resolve_local(tz, day.and_time(NaiveTime::MIN));
    let to = resolve_local(tz, (day + Duration::days(1)).and_time(NaiveTime::MIN));
    let ts =
        |t: chrono::DateTime<chrono::Utc>| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let events: Vec<CalendarEvent> = sqlx::query_as(
        "SELECT e.* FROM events e JOIN calendars c ON c.id = e.calendar_id
         WHERE e.user_id = ?1 AND e.deleted_at IS NULL AND c.enabled = 1 AND (
           e.all_day = 0 AND e.busy = 1 AND e.start_at < ?3 AND e.end_at > ?2
           OR e.all_day = 1 AND c.all_day_busy = 1 AND e.start_date <= ?4 AND e.end_date > ?4)",
    )
    .bind(&user.id)
    .bind(ts(from))
    .bind(ts(to))
    .bind(fmt(day))
    .fetch_all(conn)
    .await?;
    let mut out = vec![];
    for e in events {
        if e.all_day {
            out.push((e.id, 0, 1440));
            continue;
        }
        let p = |s: &Option<String>| {
            chrono::DateTime::parse_from_rfc3339(s.as_deref()?)
                .ok()
                .map(|t| t.to_utc())
        };
        let (Some(s), Some(en)) = (p(&e.start_at), p(&e.end_at)) else {
            continue;
        };
        let (s, en) = (s.max(from), en.min(to));
        let m =
            |t: chrono::DateTime<chrono::Utc>| ((t - from).num_minutes().max(0) as u32).min(1440);
        if s < en {
            out.push((e.id, m(s), m(en)));
        }
    }
    Ok(out)
}

pub async fn day_items(
    conn: &mut SqliteConnection,
    user: &User,
    day: NaiveDate,
) -> anyhow::Result<DayItems> {
    let date = fmt(day);
    let mut items = vec![];
    let timed: Vec<(String, String, Option<i32>, Option<i32>)> = sqlx::query_as(
        "SELECT e.id, e.start_time, e.duration_min, t.estimate_min FROM day_entries e JOIN tasks t ON t.id = e.task_id
         WHERE e.user_id = ? AND e.date = ? AND e.deleted_at IS NULL AND e.start_time IS NOT NULL
           AND t.deleted_at IS NULL AND t.status = 'open'",
    )
    .bind(&user.id)
    .bind(&date)
    .fetch_all(&mut *conn)
    .await?;
    for (id, start_time, duration, estimate) in timed {
        let Some(start) = minutes(&start_time) else {
            continue;
        };
        let dur = duration.or(estimate).unwrap_or(30).max(0) as u32;
        items.push(Item {
            id,
            kind: Kind::Task,
            start,
            end: (start + dur).min(1440),
        });
    }
    for (id, start, end) in busy_events(conn, user, day).await? {
        items.push(Item {
            id,
            kind: Kind::Event,
            start,
            end,
        });
    }
    let blocks: Vec<TimeBlock> = sqlx::query_as(
        "SELECT * FROM time_blocks WHERE user_id = ? AND date = ? AND deleted_at IS NULL",
    )
    .bind(&user.id)
    .bind(&date)
    .fetch_all(&mut *conn)
    .await?;
    for b in blocks {
        if let (Some(start), Some(end)) = (minutes(&b.start_time), minutes(&b.end_time)) {
            items.push(Item {
                id: b.id,
                kind: Kind::Block,
                start,
                end,
            });
        }
    }
    Ok(DayItems { items })
}

pub async fn day_conflicts(
    conn: &mut SqliteConnection,
    user: &User,
    day: NaiveDate,
) -> anyhow::Result<Vec<Conflict>> {
    Ok(conflicts::find(&day_items(conn, user, day).await?.items))
}

static NOTIFIED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// After the calendar changed: tell the user about new clashes between scheduled tasks
/// and events today or tomorrow (once per pair).
pub async fn notify_new_conflicts(state: &AppState, user: &User) -> anyhow::Result<()> {
    let today = today_for(user);
    let mut conn = state.db.read.acquire().await?;
    for day in [today, today + Duration::days(1)] {
        for c in day_conflicts(&mut conn, user, day).await? {
            let (task, event) = match (c.a_kind, c.b_kind) {
                (Kind::Task, Kind::Event) => (&c.a, &c.b),
                (Kind::Event, Kind::Task) => (&c.b, &c.a),
                _ => continue,
            };
            let key = format!("{}:{}:{task}:{event}", user.id, fmt(day));
            if !NOTIFIED
                .lock()
                .unwrap()
                .get_or_insert_with(HashSet::new)
                .insert(key)
            {
                continue;
            }
            let title: Option<String> = sqlx::query_scalar(
                "SELECT t.title FROM day_entries e JOIN tasks t ON t.id = e.task_id WHERE e.id = ?",
            )
            .bind(task)
            .fetch_optional(&mut *conn)
            .await?;
            let event_title: Option<String> =
                sqlx::query_scalar("SELECT title FROM events WHERE id = ?")
                    .bind(event)
                    .fetch_optional(&mut *conn)
                    .await?;
            let when = if day == today { "today" } else { "tomorrow" };
            notify::send(
                state,
                &user.id,
                &Notification {
                    kind: "conflict".into(),
                    title: format!("Overlap {when}"),
                    body: format!(
                        "“{}” overlaps “{}”",
                        title.unwrap_or_default(),
                        event_title.unwrap_or_default()
                    ),
                    url: if day == today {
                        "/".into()
                    } else {
                        format!("/day/{}", fmt(day))
                    },
                },
            );
        }
    }
    Ok(())
}

/// Tasks of a day not yet given a time, for the planner.
pub async fn unscheduled(
    conn: &mut SqliteConnection,
    user_id: &str,
    date: &str,
) -> sqlx::Result<Vec<Task>> {
    sqlx::query_as(
        "SELECT t.* FROM day_entries e JOIN tasks t ON t.id = e.task_id
         WHERE e.user_id = ? AND e.date = ? AND e.deleted_at IS NULL AND e.start_time IS NULL
           AND t.deleted_at IS NULL AND t.status = 'open'
         ORDER BY e.position",
    )
    .bind(user_id)
    .bind(date)
    .fetch_all(conn)
    .await
}
