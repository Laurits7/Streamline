//! Calendar sync (SPEC §6.5, D-55): periodically and on demand, mirror each account's
//! calendars, cache their event resources (only fetching what changed, by ctag and etag),
//! and expand the instances in a rolling window into `events`, which reach clients through
//! the change feed. On failure the last known events stay and the account shows the error.

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use chrono::{NaiveDate, NaiveTime, SecondsFormat, Utc};
use sqlx::SqliteConnection;
use streamline_domain::{
    calendar::{Instance, When, expand},
    time::{parse_tz, resolve_local},
};

use crate::{
    AppState,
    caldav::{CalDav, CalendarProvider, RemoteCalendar},
    db::next_rev,
    events::Change,
    models::{
        Calendar, CalendarAccount, CalendarAccountView, CalendarEvent, User, upsert_calendar,
        upsert_event,
    },
    rollover::today_for,
    util::{new_id, now},
};

/// Instances are kept from this many days back...
pub const WINDOW_PAST_DAYS: i64 = 30;
/// ...to this many days ahead.
pub const WINDOW_FUTURE_DAYS: i64 = 180;
/// How often accounts are synced in the background.
pub const INTERVAL: Duration = Duration::from_secs(15 * 60);

pub async fn account_for(
    conn: &mut SqliteConnection,
    user_id: &str,
) -> sqlx::Result<Option<CalendarAccount>> {
    sqlx::query_as("SELECT * FROM calendar_accounts WHERE user_id = ? AND deleted_at IS NULL ORDER BY created_at LIMIT 1")
        .bind(user_id)
        .fetch_optional(conn)
        .await
}

/// Sync one account now and record the outcome on it. Syncs run one at a time.
pub async fn sync_account(
    state: &AppState,
    account_id: &str,
    force: bool,
) -> anyhow::Result<CalendarAccount> {
    let _guard = state.calendar_lock.lock().await;
    let account: CalendarAccount = sqlx::query_as("SELECT * FROM calendar_accounts WHERE id = ?")
        .bind(account_id)
        .fetch_one(&state.db.read)
        .await?;
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(&account.user_id)
        .fetch_one(&state.db.read)
        .await?;
    let result = run(state, &account, &user, force).await;
    if let Err(e) = &result {
        tracing::info!("calendar sync failed for account {}: {e:#}", account.id);
    }
    let mut tx = state.db.write.begin().await?;
    let ts = now();
    let rev = next_rev(&mut tx).await?;
    let (status, error) = match &result {
        Ok(()) => ("ok", None),
        Err(e) => ("error", Some(format!("{e:#}"))),
    };
    sqlx::query("UPDATE calendar_accounts SET status = ?, last_error = ?, last_sync_at = ?, updated_at = ?, rev = ? WHERE id = ? AND deleted_at IS NULL")
        .bind(status)
        .bind(&error)
        .bind(&ts)
        .bind(&ts)
        .bind(rev)
        .bind(&account.id)
        .execute(&mut *tx)
        .await?;
    let updated: CalendarAccount = sqlx::query_as("SELECT * FROM calendar_accounts WHERE id = ?")
        .bind(&account.id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    if updated.deleted_at.is_none() {
        state.bus.publish([Change::calendar_account(
            &updated.user_id,
            Some(&CalendarAccountView::from(&updated)),
        )]);
    }
    Ok(updated)
}

/// The calendars an account sees, without saving anything (for "Test connection").
pub async fn probe(
    url: &str,
    username: &str,
    password: &str,
) -> anyhow::Result<Vec<RemoteCalendar>> {
    CalDav::new(url, username, password)?.list_calendars().await
}

pub fn client_for(state: &AppState, a: &CalendarAccount) -> anyhow::Result<CalDav> {
    let password = a
        .secret
        .as_deref()
        .map(|s| state.secrets.decrypt(s))
        .transpose()?
        .unwrap_or_default();
    CalDav::new(&a.url, &a.username, &password)
}

async fn run(
    state: &AppState,
    account: &CalendarAccount,
    user: &User,
    force: bool,
) -> anyhow::Result<()> {
    let client = client_for(state, account)?;
    let remote = client.list_calendars().await?;
    let today = today_for(user);

    // Mirror the calendar list.
    let mut tx = state.db.write.begin().await?;
    let mut changes = vec![];
    let existing: Vec<Calendar> = sqlx::query_as("SELECT * FROM calendars WHERE account_id = ?")
        .bind(&account.id)
        .fetch_all(&mut *tx)
        .await?;
    let ts = now();
    for r in &remote {
        let mut c = match existing.iter().find(|c| c.href == r.href) {
            Some(c) if c.deleted_at.is_none() && c.name == r.name && c.color == r.color => continue,
            Some(c) => c.clone(),
            None => Calendar {
                id: new_id(),
                account_id: account.id.clone(),
                user_id: account.user_id.clone(),
                href: r.href.clone(),
                name: String::new(),
                color: None,
                user_color: None,
                enabled: true,
                ctag: None,
                expanded_for: None,
                created_at: ts.clone(),
                updated_at: ts.clone(),
                deleted_at: None,
                rev: 0,
            },
        };
        if c.deleted_at.is_some() {
            c.deleted_at = None;
            c.ctag = None;
            c.expanded_for = None;
        }
        c.name = r.name.clone();
        c.color = r.color.clone();
        c.updated_at = ts.clone();
        c.rev = next_rev(&mut tx).await?;
        upsert_calendar(&mut tx, &c).await?;
        changes.push(Change::calendar(&c));
    }
    for c in existing
        .iter()
        .filter(|c| c.deleted_at.is_none() && !remote.iter().any(|r| r.href == c.href))
    {
        changes.extend(remove_calendar(&mut tx, c).await?);
    }
    tx.commit().await?;
    state.bus.publish(changes);

    // Fetch what changed, then re-expand.
    let calendars: Vec<Calendar> = sqlx::query_as(
        "SELECT * FROM calendars WHERE account_id = ? AND deleted_at IS NULL AND enabled = 1",
    )
    .bind(&account.id)
    .fetch_all(&state.db.read)
    .await?;
    for cal in calendars {
        let ctag = remote
            .iter()
            .find(|r| r.href == cal.href)
            .and_then(|r| r.ctag.clone());
        let fetch = force || ctag.is_none() || cal.ctag != ctag;
        let mut fetched = vec![];
        let mut removed = vec![];
        if fetch {
            let listed = client.list_objects(&cal.href).await?;
            let cached: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
                "SELECT href, etag FROM calendar_objects WHERE calendar_id = ?",
            )
            .bind(&cal.id)
            .fetch_all(&state.db.read)
            .await?
            .into_iter()
            .collect();
            let wanted: Vec<String> = listed
                .iter()
                .filter(|(h, e)| cached.get(h) != Some(e))
                .map(|(h, _)| h.clone())
                .collect();
            let present: HashSet<&String> = listed.iter().map(|(h, _)| h).collect();
            removed = cached
                .keys()
                .filter(|h| !present.contains(h))
                .cloned()
                .collect();
            fetched = client.fetch_objects(&cal.href, &wanted).await?;
        } else if cal.expanded_for.as_deref() == Some(&fmt(today)) {
            continue;
        }
        let mut tx = state.db.write.begin().await?;
        for o in &fetched {
            sqlx::query(
                "INSERT INTO calendar_objects (calendar_id, href, etag, ics) VALUES (?,?,?,?)
                 ON CONFLICT(calendar_id, href) DO UPDATE SET etag = excluded.etag, ics = excluded.ics",
            )
            .bind(&cal.id)
            .bind(&o.href)
            .bind(&o.etag)
            .bind(&o.ics)
            .execute(&mut *tx)
            .await?;
        }
        for h in &removed {
            sqlx::query("DELETE FROM calendar_objects WHERE calendar_id = ? AND href = ?")
                .bind(&cal.id)
                .bind(h)
                .execute(&mut *tx)
                .await?;
        }
        let changes = expand_calendar(&mut tx, &cal, user, today).await?;
        sqlx::query("UPDATE calendars SET ctag = ?, expanded_for = ? WHERE id = ?")
            .bind(if fetch { ctag } else { cal.ctag.clone() })
            .bind(fmt(today))
            .bind(&cal.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        state.bus.publish(changes);
    }
    Ok(())
}

fn fmt(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn ts(t: chrono::DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Tombstone a calendar's events and forget its cache (it was removed or disabled).
pub async fn clear_calendar(
    conn: &mut SqliteConnection,
    cal: &Calendar,
) -> anyhow::Result<Vec<Change>> {
    let mut changes = vec![];
    let live: Vec<CalendarEvent> =
        sqlx::query_as("SELECT * FROM events WHERE calendar_id = ? AND deleted_at IS NULL")
            .bind(&cal.id)
            .fetch_all(&mut *conn)
            .await?;
    let t = now();
    for mut e in live {
        e.deleted_at = Some(t.clone());
        e.updated_at = t.clone();
        e.rev = next_rev(conn).await?;
        upsert_event(conn, &e).await?;
        changes.push(Change::event(&e));
    }
    sqlx::query("DELETE FROM calendar_objects WHERE calendar_id = ?")
        .bind(&cal.id)
        .execute(&mut *conn)
        .await?;
    sqlx::query("UPDATE calendars SET ctag = NULL, expanded_for = NULL WHERE id = ?")
        .bind(&cal.id)
        .execute(&mut *conn)
        .await?;
    Ok(changes)
}

pub async fn remove_calendar(
    conn: &mut SqliteConnection,
    cal: &Calendar,
) -> anyhow::Result<Vec<Change>> {
    let mut changes = clear_calendar(conn, cal).await?;
    let mut c = cal.clone();
    c.deleted_at = Some(now());
    c.updated_at = now();
    c.ctag = None;
    c.expanded_for = None;
    c.rev = next_rev(conn).await?;
    upsert_calendar(conn, &c).await?;
    changes.push(Change::calendar(&c));
    Ok(changes)
}

fn row_of(i: &Instance, cal: &Calendar) -> CalendarEvent {
    let (all_day, start_at, end_at, start_date, end_date) = match &i.when {
        When::Timed { start, end } => (false, Some(ts(*start)), Some(ts(*end)), None, None),
        When::AllDay { start, end } => (true, None, None, Some(fmt(*start)), Some(fmt(*end))),
    };
    CalendarEvent {
        id: String::new(),
        user_id: cal.user_id.clone(),
        calendar_id: cal.id.clone(),
        uid: i.uid.clone(),
        instance_key: i.key.clone(),
        title: i.title.clone(),
        location: i.location.clone(),
        all_day,
        start_at,
        end_at,
        start_date,
        end_date,
        busy: i.busy,
        recurring: i.recurring,
        updated_at: String::new(),
        deleted_at: None,
        rev: 0,
    }
}

/// Recompute the calendar's instances in the window around `today` from its cached
/// resources, writing only what changed. Instances before the window are left alone.
pub async fn expand_calendar(
    conn: &mut SqliteConnection,
    cal: &Calendar,
    user: &User,
    today: NaiveDate,
) -> anyhow::Result<Vec<Change>> {
    let tz = parse_tz(&user.timezone).unwrap_or(chrono_tz::UTC);
    let first = today - chrono::Duration::days(WINDOW_PAST_DAYS);
    let from = resolve_local(tz, first.and_time(NaiveTime::MIN));
    let to = resolve_local(
        tz,
        (today + chrono::Duration::days(WINDOW_FUTURE_DAYS + 1)).and_time(NaiveTime::MIN),
    );

    let objects: Vec<(String, String)> =
        sqlx::query_as("SELECT href, ics FROM calendar_objects WHERE calendar_id = ?")
            .bind(&cal.id)
            .fetch_all(&mut *conn)
            .await?;
    let mut wanted: HashMap<(String, String), CalendarEvent> = HashMap::new();
    for (href, ics) in &objects {
        match expand(ics, tz, from, to) {
            Ok(x) => {
                for w in &x.warnings {
                    tracing::debug!("{href}: {w}");
                }
                for i in &x.instances {
                    wanted.insert((i.uid.clone(), i.key.clone()), row_of(i, cal));
                }
            }
            Err(e) => tracing::info!("skipping unreadable calendar object {href}: {e}"),
        }
    }

    // Everything stored that overlaps the window (deleted rows too, so they can come back).
    let stored: Vec<CalendarEvent> = sqlx::query_as(
        "SELECT * FROM events WHERE calendar_id = ?1 AND (all_day = 0 AND end_at >= ?2 OR all_day = 1 AND end_date > ?3)",
    )
    .bind(&cal.id)
    .bind(ts(from))
    .bind(fmt(first))
    .fetch_all(&mut *conn)
    .await?;
    let mut stored: HashMap<(String, String), CalendarEvent> = stored
        .into_iter()
        .map(|e| ((e.uid.clone(), e.instance_key.clone()), e))
        .collect();

    let mut changes = vec![];
    let t = now();
    for (key, mut row) in wanted {
        let existing =
            match stored.remove(&key) {
                Some(e) => Some(e),
                // Stored with times outside the window (the event moved into it).
                None => sqlx::query_as(
                    "SELECT * FROM events WHERE calendar_id = ? AND uid = ? AND instance_key = ?",
                )
                .bind(&cal.id)
                .bind(&key.0)
                .bind(&key.1)
                .fetch_optional(&mut *conn)
                .await?,
            };
        if let Some(e) = &existing {
            row.id = e.id.clone();
            row.updated_at = e.updated_at.clone();
            row.rev = e.rev;
            if &row == e {
                continue;
            }
        } else {
            row.id = new_id();
        }
        row.updated_at = t.clone();
        row.rev = next_rev(conn).await?;
        upsert_event(conn, &row).await?;
        changes.push(Change::event(&row));
    }
    for (_, mut gone) in stored.into_iter().filter(|(_, e)| e.deleted_at.is_none()) {
        gone.deleted_at = Some(t.clone());
        gone.updated_at = t.clone();
        gone.rev = next_rev(conn).await?;
        upsert_event(conn, &gone).await?;
        changes.push(Change::event(&gone));
    }
    Ok(changes)
}

/// Background job: sync accounts whose last sync is older than [`INTERVAL`].
pub async fn run_due(state: &AppState) -> anyhow::Result<()> {
    let cutoff = (Utc::now() - chrono::Duration::from_std(INTERVAL)?)
        .to_rfc3339_opts(SecondsFormat::Millis, true);
    let due: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM calendar_accounts WHERE deleted_at IS NULL AND (last_sync_at IS NULL OR last_sync_at < ?)",
    )
    .bind(cutoff)
    .fetch_all(&state.db.read)
    .await?;
    for id in due {
        sync_account(state, &id, false).await?;
    }
    Ok(())
}

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            if let Err(e) = run_due(&state).await {
                tracing::warn!("calendar sync job failed: {e:#}");
            }
        }
    });
}
