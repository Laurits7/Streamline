//! Time blocks, day templates and conflicts (SPEC §6.8, §6.11; Phase 5a).
//! Weekday templates are applied to the coming week's days once (`block_days` records
//! it), so later edits and deletions stick. Conflicts are computed for a local calendar
//! day from timed tasks, busy events and blocks (`domain::conflicts`).

use std::{
    collections::{BTreeMap, HashSet},
    sync::Mutex,
};

use chrono::{Duration, NaiveDate, NaiveTime, Timelike};
use sqlx::SqliteConnection;
use streamline_domain::{
    conflicts::{self, Conflict, Item, Kind},
    order::key_after,
    templates::{Week, WeekMode, day_template_for, lay_out},
    time::{parse_hhmm, parse_tz, resolve_local},
};

use crate::{
    AppState,
    db::next_rev,
    events::Change,
    models::{
        CalendarEvent, DayEntry, DayTemplate, Series, Task, TemplateItem, TimeBlock, User,
        WeekTemplate, upsert_day_template, upsert_entry, upsert_task, upsert_time_block,
        upsert_week_template,
    },
    notify::{self, Notification},
    rollover::today_for,
    util::{new_id, now},
    visibility::OWNED_VISIBLE_SQL,
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
/// template the day already has is a no-op, unless `force` (the template changed).
pub async fn apply(
    conn: &mut SqliteConnection,
    user_id: &str,
    date: &str,
    tpl: Option<&DayTemplate>,
    force: bool,
) -> anyhow::Result<Vec<Change>> {
    let existing: Vec<TimeBlock> = sqlx::query_as(
        "SELECT * FROM time_blocks WHERE user_id = ? AND date = ? AND deleted_at IS NULL",
    )
    .bind(user_id)
    .bind(date)
    .fetch_all(&mut *conn)
    .await?;
    if let Some(t) = tpl
        && !force
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

/// Plan the routines and one-off tasks of the template's blocks on the day (D-113): each
/// block's items one after another from its start, around what's already scheduled; what
/// doesn't fit is planned without a time. Each item is planned once per day (recorded in
/// `block_days.placed`): a day set up again keeps what it got, wherever the user moved it,
/// and only gets the items that are new.
pub async fn place_items(
    conn: &mut SqliteConnection,
    user: &User,
    day: NaiveDate,
    tpl: &DayTemplate,
) -> anyhow::Result<Vec<Change>> {
    if tpl.blocks.0.iter().all(|b| b.items.is_empty()) {
        return Ok(vec![]);
    }
    let date = fmt(day);
    let mut placed: BTreeMap<String, String> = sqlx::query_scalar::<_, String>(
        "SELECT placed FROM block_days WHERE user_id = ? AND date = ?",
    )
    .bind(&user.id)
    .bind(&date)
    .fetch_optional(&mut *conn)
    .await?
    .and_then(|p| serde_json::from_str(&p).ok())
    .unwrap_or_default();
    let mut busy: Vec<(u32, u32)> = day_items(conn, user, day)
        .await?
        .items
        .into_iter()
        .filter(|i| i.kind != Kind::Block)
        .map(|i| (i.start, i.end))
        .collect();
    let mut changes = vec![];
    for b in &tpl.blocks.0 {
        let (Some(start), Some(end)) = (minutes(&b.start), minutes(&b.end)) else {
            continue;
        };
        // (item, its task, whether the task is new)
        let mut todo: Vec<(String, Task, bool)> = vec![];
        for it in &b.items {
            if placed.contains_key(&it.id) {
                continue;
            }
            match it.kind.as_str() {
                "routine" => {
                    let taken: Vec<&String> = placed.values().collect();
                    if let Some(t) = routine_occurrence(
                        conn,
                        user,
                        it.series_id.as_deref().unwrap_or_default(),
                        day,
                        &taken,
                        &mut changes,
                    )
                    .await?
                    {
                        todo.push((it.id.clone(), t, false));
                    }
                }
                "task" => {
                    let key = format!("{}:{date}", it.id);
                    // Made before (even if deleted or moved since): never again.
                    let existing: Option<String> = sqlx::query_scalar(
                        "SELECT id FROM tasks WHERE ext_source = 'template' AND ext_id = ? ORDER BY deleted_at IS NULL DESC LIMIT 1",
                    )
                    .bind(&key)
                    .fetch_optional(&mut *conn)
                    .await?;
                    if let Some(id) = existing {
                        placed.insert(it.id.clone(), id);
                        continue;
                    }
                    todo.push((it.id.clone(), one_off(conn, user, it, key).await?, true));
                }
                _ => {}
            }
        }
        let dur = |t: &Task| t.estimate_min.filter(|&m| m > 0).unwrap_or(30) as u32;
        let durations: Vec<u32> = todo.iter().map(|(_, t, _)| dur(t)).collect();
        let times = lay_out((start, end), &busy, &durations);
        for ((item_id, mut t, new), at) in todo.into_iter().zip(times) {
            let rev = next_rev(conn).await?;
            if new {
                t.rev = rev;
                upsert_task(conn, &t).await?;
                changes.push(Change::task(&t));
            }
            if let Some(s) = at {
                busy.push((s, s + dur(&t)));
            }
            let last: Option<String> = sqlx::query_scalar(
                "SELECT MAX(position) FROM day_entries WHERE user_id = ? AND date = ? AND deleted_at IS NULL",
            )
            .bind(&user.id)
            .bind(&date)
            .fetch_one(&mut *conn)
            .await?;
            let ts = now();
            let e = DayEntry {
                id: new_id(),
                user_id: user.id.clone(),
                date: date.clone(),
                task_id: t.id.clone(),
                position: key_after(last.as_deref()),
                start_time: at.map(hhmm),
                duration_min: None,
                created_at: ts.clone(),
                updated_at: ts,
                deleted_at: None,
                rev,
            };
            upsert_entry(conn, &e).await?;
            changes.push(Change::entry(&e));
            placed.insert(item_id, t.id);
        }
    }
    sqlx::query(
        "INSERT INTO block_days (user_id, date, placed) VALUES (?, ?, ?)
         ON CONFLICT(user_id, date) DO UPDATE SET placed = excluded.placed",
    )
    .bind(&user.id)
    .bind(&date)
    .bind(serde_json::to_string(&placed)?)
    .execute(&mut *conn)
    .await?;
    Ok(changes)
}

/// The open occurrence of a routine (or of its current version, if it was changed since
/// it went into the template) for a day: the one due that day, or for an N-per-week/month
/// routine a slot of the window containing the day. Occurrences already planned (from
/// today on), or `taken` by this day's other items, don't count: the user's own plan stays.
async fn routine_occurrence(
    conn: &mut SqliteConnection,
    user: &User,
    series_id: &str,
    day: NaiveDate,
    taken: &[&String],
    changes: &mut Vec<Change>,
) -> anyhow::Result<Option<Task>> {
    let id = crate::routines::current_version(conn, series_id).await?;
    let s: Option<Series> = sqlx::query_as(&format!(
        "SELECT * FROM series WHERE {OWNED_VISIBLE_SQL} AND id = ?2 AND deleted_at IS NULL"
    ))
    .bind(&user.id)
    .bind(&id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(s) = s else { return Ok(None) };
    let today = today_for(user);
    // Occurrences exist up to tomorrow; days further ahead get theirs now.
    if day > today + Duration::days(1) {
        changes.extend(
            crate::routines::materialize_series(conn, &s, user.week_start as u32, today, day, day)
                .await?,
        );
    }
    let ids = crate::routines::lineage(conn, &s).await?;
    Ok(sqlx::query_as(
        "SELECT t.* FROM tasks t
         WHERE t.series_id IN (SELECT value FROM json_each(?1)) AND t.deleted_at IS NULL AND t.status = 'open'
           AND (t.occurrence_date = ?2 OR t.window_end IS NOT NULL AND t.occurrence_date <= ?2 AND t.window_end >= ?2)
           AND t.id NOT IN (SELECT value FROM json_each(?5))
           AND NOT EXISTS (SELECT 1 FROM day_entries e WHERE e.task_id = t.id AND e.user_id = ?3
                             AND e.deleted_at IS NULL AND e.date >= ?4)
         ORDER BY t.occurrence_key LIMIT 1",
    )
    .bind(serde_json::to_string(&ids)?)
    .bind(fmt(day))
    .bind(&user.id)
    .bind(fmt(today))
    .bind(serde_json::to_string(taken)?)
    .fetch_optional(&mut *conn)
    .await?)
}

/// A one-off item's task for one day (not saved; `rev` is set when it is).
async fn one_off(
    conn: &mut SqliteConnection,
    user: &User,
    it: &TemplateItem,
    key: String,
) -> sqlx::Result<Task> {
    let last: Option<String> = sqlx::query_scalar(
        "SELECT MAX(position) FROM tasks WHERE project_id IS NULL AND owner_user_id = ? AND deleted_at IS NULL",
    )
    .bind(&user.id)
    .fetch_one(&mut *conn)
    .await?;
    let ts = now();
    Ok(Task {
        id: new_id(),
        owner_user_id: Some(user.id.clone()),
        owner_group_id: None,
        assignee_user_id: None,
        project_id: None,
        title: it.title.clone().unwrap_or_default(),
        notes: it.notes.clone().unwrap_or_default(),
        status: "open".into(),
        position: key_after(last.as_deref()),
        due_date: None,
        estimate_min: it.estimate_min,
        difficulty: None,
        importance: None,
        urgency: None,
        actual_min: 0,
        task_type_id: crate::routes::tasks::DEFAULT_TASK_TYPE.into(),
        carry_count: 0,
        started_at: None,
        completed_at: None,
        completed_by: None,
        ext_source: Some("template".into()),
        ext_id: Some(key),
        ext_url: None,
        place_id: None,
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
        ongoing: false,
        start_date: None,
        worked_days: sqlx::types::Json(vec![]),
        created_at: ts.clone(),
        updated_at: ts,
        deleted_at: None,
        rev: 0,
    })
}

/// A week template for `domain::templates` (`None` if its mode or dates don't parse).
pub fn week(w: &WeekTemplate) -> Option<Week> {
    let mut days: [Option<String>; 7] = Default::default();
    for (d, id) in days.iter_mut().zip(w.days.0.iter()) {
        d.clone_from(id);
    }
    let date = |s: &Option<String>| {
        s.as_deref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
    };
    Some(Week {
        id: w.id.clone(),
        mode: WeekMode::parse(&w.mode)?,
        days,
        start: date(&w.start_date),
        end: date(&w.end_date),
    })
}

pub async fn week_templates(
    conn: &mut SqliteConnection,
    user_id: &str,
) -> sqlx::Result<Vec<WeekTemplate>> {
    sqlx::query_as(
        "SELECT * FROM week_templates WHERE owner_user_id = ? AND deleted_at IS NULL ORDER BY position, id",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}

/// The default week changed: day templates' `weekdays` follow it (for older clients).
pub async fn weekdays_from_default(
    conn: &mut SqliteConnection,
    user_id: &str,
    changes: &mut Vec<Change>,
) -> sqlx::Result<()> {
    let weeks = week_templates(conn, user_id).await?;
    let days = weeks
        .iter()
        .find(|w| w.mode == "default")
        .map(|w| w.days.0.clone())
        .unwrap_or_default();
    let templates: Vec<DayTemplate> = sqlx::query_as(
        "SELECT * FROM day_templates WHERE owner_user_id = ? AND deleted_at IS NULL",
    )
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await?;
    for mut t in templates {
        let want: Vec<i32> = (1..=7)
            .filter(|&d| days.get(d as usize - 1).cloned().flatten().as_deref() == Some(&t.id))
            .collect();
        if t.weekdays.0 != want {
            t.weekdays = sqlx::types::Json(want);
            t.updated_at = now();
            t.rev = next_rev(conn).await?;
            upsert_day_template(conn, &t).await?;
            changes.push(Change::day_template(&t));
        }
    }
    Ok(())
}

/// Day templates' `weekdays` changed (an older client, or the Settings page): the
/// default week follows, and is made if there is none yet.
pub async fn default_from_weekdays(
    conn: &mut SqliteConnection,
    user_id: &str,
    changes: &mut Vec<Change>,
) -> sqlx::Result<()> {
    let templates: Vec<DayTemplate> = sqlx::query_as(
        "SELECT * FROM day_templates WHERE owner_user_id = ? AND deleted_at IS NULL ORDER BY position",
    )
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await?;
    let days: Vec<Option<String>> = (1..=7)
        .map(|d| {
            templates
                .iter()
                .find(|t| t.weekdays.0.contains(&d))
                .map(|t| t.id.clone())
        })
        .collect();
    let weeks = week_templates(conn, user_id).await?;
    let ts = now();
    let mut w = match weeks.iter().find(|w| w.mode == "default") {
        Some(w) if w.days.0 == days => return Ok(()),
        Some(w) => w.clone(),
        None if days.iter().all(Option::is_none) => return Ok(()),
        None => WeekTemplate {
            id: new_id(),
            owner_user_id: user_id.into(),
            name: "Usual week".into(),
            days: sqlx::types::Json(vec![]),
            mode: "default".into(),
            start_date: None,
            end_date: None,
            position: key_after(weeks.last().map(|w| w.position.as_str())),
            created_at: ts.clone(),
            updated_at: ts.clone(),
            deleted_at: None,
            rev: 0,
        },
    };
    w.days = sqlx::types::Json(days);
    w.updated_at = ts;
    w.rev = next_rev(conn).await?;
    upsert_week_template(conn, &w).await?;
    changes.push(Change::week_template(&w));
    Ok(())
}

/// Keep the user's next 8 days in line with their week templates, except days whose
/// blocks were changed by hand.
pub async fn materialize_user(state: &AppState, user: &User) -> anyhow::Result<()> {
    let templates: Vec<DayTemplate> =
        sqlx::query_as("SELECT * FROM day_templates WHERE owner_user_id = ? AND deleted_at IS NULL ORDER BY position")
            .bind(&user.id)
            .fetch_all(&state.db.read)
            .await?;
    let weeks: Vec<Week> = week_templates(&mut *state.db.read.acquire().await?, &user.id)
        .await?
        .iter()
        .filter_map(week)
        .collect();
    let today = today_for(user);
    // Nothing to keep in line: no week templates and no auto-set-up days ahead.
    if weeks.iter().all(|w| w.days.iter().all(Option::is_none)) {
        let auto: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM block_days WHERE user_id = ? AND manual = 0 AND template_id IS NOT NULL AND date >= ?)",
        )
        .bind(&user.id)
        .bind(fmt(today))
        .fetch_one(&state.db.read)
        .await?;
        if !auto {
            return Ok(());
        }
    }
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
        let want =
            day_template_for(day, &weeks).and_then(|id| templates.iter().find(|t| t.id == id));
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
        changes.extend(apply(&mut tx, &user.id, &date, want, true).await?);
        if let Some(t) = want {
            changes.extend(place_items(&mut tx, user, day, t).await?);
        }
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
        "SELECT * FROM users WHERE deleted_at IS NULL AND home IS NULL AND (id IN (SELECT owner_user_id FROM week_templates WHERE deleted_at IS NULL)
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
         WHERE e.user_id = ?1 AND e.date = ?2 AND e.deleted_at IS NULL AND e.start_time IS NOT NULL
           AND t.deleted_at IS NULL AND t.status = 'open'
           -- waiting for results takes no time (D-70)
           AND t.waiting_since IS NULL
           AND (t.owner_user_id = ?1 OR t.owner_group_id IN (SELECT group_id FROM group_members WHERE user_id = ?1))",
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
         WHERE e.user_id = ?1 AND e.date = ?2 AND e.deleted_at IS NULL AND e.start_time IS NULL
           AND t.deleted_at IS NULL AND t.status = 'open' AND t.waiting_since IS NULL
           AND (t.owner_user_id = ?1 OR t.owner_group_id IN (SELECT group_id FROM group_members WHERE user_id = ?1))
         ORDER BY e.position",
    )
    .bind(user_id)
    .bind(date)
    .fetch_all(conn)
    .await
}
