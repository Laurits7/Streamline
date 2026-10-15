//! Calendar write-back (7.7, D-98): tasks with a time, from yesterday to two weeks ahead,
//! written as busy events to the calendar the user chose, so their calendar shows the time
//! is taken. Each run compares with what was written last and only sends changes; events
//! Streamline wrote are skipped when the calendar is read back (`OWN_UID_PREFIX`).

use chrono::{Duration, NaiveDate, NaiveDateTime};
use streamline_domain::time::{parse_hhmm, parse_tz, resolve_local};

use crate::{
    AppState,
    caldav::CalendarProvider,
    models::{CalendarAccount, User},
};

/// UIDs of events Streamline writes; the calendar sync leaves them out.
pub const OWN_UID_PREFIX: &str = "streamline-entry-";
/// Days written: from yesterday to two weeks ahead.
const BACK: i64 = 1;
const AHEAD: i64 = 14;
/// At most this many calendar writes per account and run (the rest follow next minute).
const MAX_WRITES: usize = 50;

/// RFC 5545 text: backslash, comma, semicolon and line breaks escaped.
fn text(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
        .replace('\r', "")
}

/// The event for one timed task.
pub fn ics(
    entry_id: &str,
    start: chrono::DateTime<chrono::Utc>,
    minutes: i64,
    summary: &str,
) -> String {
    let fmt = |t: chrono::DateTime<chrono::Utc>| t.format("%Y%m%dT%H%M%SZ").to_string();
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Streamline//Write-back//EN\r\nBEGIN:VEVENT\r\nUID:{OWN_UID_PREFIX}{entry_id}@streamline\r\nDTSTAMP:{}\r\nDTSTART:{}\r\nDTEND:{}\r\nSUMMARY:{}\r\nTRANSP:OPAQUE\r\nX-STREAMLINE:1\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        fmt(chrono::Utc::now()),
        fmt(start),
        fmt(start + Duration::minutes(minutes)),
        text(summary)
    )
}

#[derive(sqlx::FromRow)]
struct Timed {
    entry_id: String,
    date: String,
    start_time: String,
    minutes: i64,
    title: String,
}

#[derive(sqlx::FromRow)]
struct Written {
    entry_id: String,
    calendar_id: String,
    href: String,
    hash: String,
}

/// Bring one account's written events in line with the plan.
pub async fn sync(state: &AppState, account: &CalendarAccount) -> anyhow::Result<()> {
    let written: Vec<Written> = sqlx::query_as(
        "SELECT entry_id, calendar_id, href, hash FROM written_events WHERE account_id = ?",
    )
    .bind(&account.id)
    .fetch_all(&state.db.read)
    .await?;
    let target: Option<(String, String)> = match &account.write_calendar_id {
        Some(id) => sqlx::query_as(
            "SELECT id, href FROM calendars WHERE id = ? AND account_id = ? AND deleted_at IS NULL",
        )
        .bind(id)
        .bind(&account.id)
        .fetch_optional(&state.db.read)
        .await?,
        None => None,
    };
    if target.is_none() && written.is_empty() {
        return Ok(());
    }
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(&account.user_id)
        .fetch_one(&state.db.read)
        .await?;
    let today: NaiveDate = crate::rollover::today_for(&user);
    let (from, to) = (today - Duration::days(BACK), today + Duration::days(AHEAD));
    let fmt = |d: NaiveDate| d.format("%Y-%m-%d").to_string();

    // What should be in the calendar now.
    let wanted: Vec<Timed> = if target.is_some() {
        sqlx::query_as(
            "SELECT e.id AS entry_id, e.date, e.start_time, COALESCE(e.duration_min, t.estimate_min, 30) AS minutes, t.title
               FROM day_entries e JOIN tasks t ON t.id = e.task_id
              WHERE e.user_id = ?1 AND e.deleted_at IS NULL AND e.start_time IS NOT NULL AND e.date >= ?2 AND e.date <= ?3
                AND t.deleted_at IS NULL AND t.status IN ('open', 'done')",
        )
        .bind(&user.id)
        .bind(fmt(from))
        .bind(fmt(to))
        .fetch_all(&state.db.read)
        .await?
    } else {
        vec![]
    };
    let tz = parse_tz(&user.timezone).unwrap_or(chrono_tz::UTC);
    let client = crate::calsync::client_for(state, account)?;
    let mut writes = 0;

    // Removed: no longer wanted (unscheduled, deleted, skipped, moved out of range, or the
    // calendar changed / write-back turned off). Older history is left alone.
    for w in &written {
        let still = wanted.iter().any(|t| t.entry_id == w.entry_id)
            && target.as_ref().is_some_and(|(id, _)| *id == w.calendar_id);
        if still {
            continue;
        }
        let entry_date: Option<String> =
            sqlx::query_scalar("SELECT date FROM day_entries WHERE id = ? AND deleted_at IS NULL")
                .bind(&w.entry_id)
                .fetch_optional(&state.db.read)
                .await?;
        let in_range = entry_date
            .as_deref()
            .is_none_or(|d| d >= fmt(from).as_str());
        let calendar_moved = target.as_ref().is_none_or(|(id, _)| *id != w.calendar_id);
        if !(in_range || calendar_moved) {
            continue;
        }
        if writes >= MAX_WRITES {
            break;
        }
        client.delete_event(&w.href).await?;
        writes += 1;
        sqlx::query("DELETE FROM written_events WHERE account_id = ? AND entry_id = ?")
            .bind(&account.id)
            .bind(&w.entry_id)
            .execute(&state.db.write)
            .await?;
    }

    // Added or changed.
    let Some((cal_id, cal_href)) = target else {
        return Ok(());
    };
    for t in &wanted {
        let (Ok(day), Some(at)) = (
            NaiveDate::parse_from_str(&t.date, "%Y-%m-%d"),
            parse_hhmm(&t.start_time),
        ) else {
            continue;
        };
        let start = resolve_local(tz, NaiveDateTime::new(day, at));
        let summary = if account.write_titles {
            t.title.as_str()
        } else {
            "Busy"
        };
        let hash =
            crate::util::sha256_hex(&format!("{cal_id}|{start}|{}|{summary}", t.minutes.max(5)));
        if written
            .iter()
            .any(|w| w.entry_id == t.entry_id && w.hash == hash && w.calendar_id == cal_id)
        {
            continue;
        }
        if writes >= MAX_WRITES {
            break;
        }
        let href = client
            .put_event(
                &cal_href,
                &format!("streamline-{}.ics", t.entry_id),
                &ics(&t.entry_id, start, t.minutes.max(5), summary),
            )
            .await?;
        writes += 1;
        sqlx::query(
            "INSERT INTO written_events (account_id, entry_id, calendar_id, href, hash) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(account_id, entry_id) DO UPDATE SET calendar_id = ?3, href = ?4, hash = ?5",
        )
        .bind(&account.id)
        .bind(&t.entry_id)
        .bind(&cal_id)
        .bind(&href)
        .bind(&hash)
        .execute(&state.db.write)
        .await?;
    }
    Ok(())
}

/// Every account with write-back on (or events still to take back).
pub async fn run_all(state: &AppState) -> anyhow::Result<()> {
    let accounts: Vec<CalendarAccount> = sqlx::query_as(
        "SELECT * FROM calendar_accounts a WHERE a.deleted_at IS NULL AND a.status != 'error'
           AND (a.write_calendar_id IS NOT NULL OR EXISTS (SELECT 1 FROM written_events w WHERE w.account_id = a.id))",
    )
    .fetch_all(&state.db.read)
    .await?;
    for a in accounts {
        if let Err(e) = sync(state, &a).await {
            tracing::info!("calendar write-back failed for account {}: {e:#}", a.id);
        }
    }
    Ok(())
}

/// Take back everything written (the account is being removed).
pub async fn remove_all(state: &AppState, account: &CalendarAccount) -> anyhow::Result<()> {
    let written: Vec<Written> = sqlx::query_as(
        "SELECT entry_id, calendar_id, href, hash FROM written_events WHERE account_id = ?",
    )
    .bind(&account.id)
    .fetch_all(&state.db.read)
    .await?;
    let client = crate::calsync::client_for(state, account)?;
    for w in written {
        client.delete_event(&w.href).await?;
    }
    sqlx::query("DELETE FROM written_events WHERE account_id = ?")
        .bind(&account.id)
        .execute(&state.db.write)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_plain_busy_event() {
        let start = "2026-10-15T07:30:00Z".parse().unwrap();
        let ics = ics("01ABC", start, 45, "Call plumber, re: tap; now\nplease");
        assert!(ics.contains("UID:streamline-entry-01ABC@streamline\r\n"));
        assert!(ics.contains("DTSTART:20261015T073000Z\r\nDTEND:20261015T081500Z\r\n"));
        assert!(ics.contains("SUMMARY:Call plumber\\, re: tap\\; now\\nplease\r\n"));
        assert!(ics.contains("TRANSP:OPAQUE"));
    }
}
