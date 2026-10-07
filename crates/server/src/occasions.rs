//! Occasions (SPEC §6.17, D-57): the shared nameday calendar, and the tasks each user's
//! people of interest bring (e.g. buy a present 2 days before → wish happy birthday).
//! Tasks are created a week before an occasion's first step, once each: the key
//! `person:kind:date:step` is kept in `tasks.ext_id` (source `occasion`).

use std::{collections::HashMap, sync::Mutex, time::Instant};

use anyhow::{Context, bail};
use chrono::{Datelike, NaiveDate};
use sqlx::SqliteConnection;
use streamline_domain::{
    occasions::{Nameday, Step, due_occasions, fold, parse_stat_ee},
    order::key_after,
};

use crate::{
    AppState,
    db::next_rev,
    events::Change,
    models::{
        OccasionStep, OccasionTemplate, Person, Task, User, upsert_occasion_template, upsert_task,
    },
    rollover::today_for,
    util::{new_id, now},
};

/// The official Estonian nameday list (Statistics Estonia, from P. Mäeniit "Eesti nimed").
pub const STAT_EE_URL: &str = "https://www.stat.ee/nimed/NIMEPAEVAD";
/// Tasks appear this many days before an occasion's first step.
pub const LEAD_DAYS: i64 = 7;

pub fn default_steps(kind: &str) -> Vec<OccasionStep> {
    let step = |title: &str, offset_days, task_type_id: &str, after_previous| OccasionStep {
        title: title.into(),
        offset_days,
        task_type_id: task_type_id.into(),
        after_previous,
    };
    match kind {
        "birthday" => vec![
            step("Buy a present for {name}", -2, "tt_deadline", false),
            step("Wish {name} a happy birthday", 0, "tt_expires", true),
        ],
        _ => vec![step("Wish {name} a happy nameday", 0, "tt_expires", false)],
    }
}

pub const KINDS: [&str; 2] = ["nameday", "birthday"];

/// The user's templates, creating the defaults the first time.
pub async fn templates(
    conn: &mut SqliteConnection,
    user_id: &str,
    changes: &mut Vec<Change>,
) -> anyhow::Result<Vec<OccasionTemplate>> {
    let mut have: Vec<OccasionTemplate> =
        sqlx::query_as("SELECT * FROM occasion_templates WHERE owner_user_id = ?")
            .bind(user_id)
            .fetch_all(&mut *conn)
            .await?;
    for kind in KINDS {
        if have.iter().any(|t| t.kind == kind) {
            continue;
        }
        let t = OccasionTemplate {
            id: new_id(),
            owner_user_id: user_id.into(),
            kind: kind.into(),
            enabled: true,
            steps: sqlx::types::Json(default_steps(kind)),
            updated_at: now(),
            rev: next_rev(conn).await?,
        };
        upsert_occasion_template(conn, &t).await?;
        changes.push(Change::occasion_template(&t));
        have.push(t);
    }
    Ok(have)
}

/// (month, day) of a birthday stored as `YYYY-MM-DD` or `--MM-DD`, with the year if known.
pub fn birthday_parts(b: &str) -> Option<(u32, u32, Option<i32>)> {
    if let Some(md) = b.strip_prefix("--") {
        let (m, d) = md.split_once('-')?;
        let (m, d) = (m.parse().ok()?, d.parse().ok()?);
        return NaiveDate::from_ymd_opt(2024, m, d).map(|_| (m, d, None));
    }
    let date = NaiveDate::parse_from_str(b, "%Y-%m-%d").ok()?;
    Some((date.month(), date.day(), Some(date.year())))
}

pub async fn nameday_dates(
    conn: &mut SqliteConnection,
    name: &str,
) -> sqlx::Result<Vec<(u32, u32)>> {
    sqlx::query_as::<_, (i64, i64)>(
        "SELECT month, day FROM namedays WHERE name_key = ? ORDER BY month, day",
    )
    .bind(fold(name))
    .fetch_all(conn)
    .await
    .map(|v| v.into_iter().map(|(m, d)| (m as u32, d as u32)).collect())
}

/// Create the tasks of the user's upcoming occasions that don't exist yet.
pub async fn materialize_user(state: &AppState, user: &User) -> anyhow::Result<()> {
    let people: Vec<Person> =
        sqlx::query_as("SELECT * FROM people WHERE owner_user_id = ? AND deleted_at IS NULL")
            .bind(&user.id)
            .fetch_all(&state.db.read)
            .await?;
    if people.is_empty() {
        return Ok(());
    }
    let today = today_for(user);
    // Skip when nothing changed since the last run today (people, templates, calendar).
    let stamp: (Option<i64>, Option<i64>, Option<String>) = sqlx::query_as(
        "SELECT (SELECT MAX(rev) FROM people WHERE owner_user_id = ?1), (SELECT MAX(rev) FROM occasion_templates WHERE owner_user_id = ?1),
                (SELECT loaded_at FROM nameday_source WHERE id = 1)",
    )
    .bind(&user.id)
    .fetch_one(&state.db.read)
    .await?;
    let key = format!("{today}|{:?}", stamp);
    if CHECKED.lock().unwrap().get(&user.id) == Some(&key) {
        return Ok(());
    }
    let mut tx = state.db.write.begin().await?;
    let mut changes = vec![];
    let templates = templates(&mut tx, &user.id, &mut changes).await?;
    for p in &people {
        for tpl in templates
            .iter()
            .filter(|t| t.enabled && !t.steps.0.is_empty())
        {
            let (dates, born) = match tpl.kind.as_str() {
                "nameday" => match &p.nameday_name {
                    Some(n) => (nameday_dates(&mut tx, n).await?, None),
                    None => continue,
                },
                _ => match p.birthday.as_deref().and_then(birthday_parts) {
                    Some((m, d, y)) => (vec![(m, d)], y),
                    None => continue,
                },
            };
            let steps: Vec<Step> = tpl
                .steps
                .0
                .iter()
                .map(|s| Step {
                    offset_days: s.offset_days,
                })
                .collect();
            for (date, step_dates) in due_occasions(&dates, &steps, today, LEAD_DAYS) {
                create_occasion(&mut tx, user, p, tpl, date, &step_dates, born, &mut changes)
                    .await?;
            }
        }
    }
    tx.commit().await?;
    state.bus.publish(changes);
    CHECKED.lock().unwrap().insert(user.id.clone(), key);
    Ok(())
}

/// Per user: the state last materialized (see `materialize_user`).
static CHECKED: std::sync::LazyLock<Mutex<HashMap<String, String>>> =
    std::sync::LazyLock::new(Default::default);

#[allow(clippy::too_many_arguments)]
async fn create_occasion(
    conn: &mut SqliteConnection,
    user: &User,
    p: &Person,
    tpl: &OccasionTemplate,
    date: NaiveDate,
    step_dates: &[NaiveDate],
    born: Option<i32>,
    changes: &mut Vec<Change>,
) -> anyhow::Result<()> {
    let what = match (tpl.kind.as_str(), born) {
        ("birthday", Some(y)) => format!(
            "{}'s birthday on {} (turns {})",
            p.name,
            date.format("%-d %B"),
            date.year() - y
        ),
        ("birthday", None) => format!("{}'s birthday on {}", p.name, date.format("%-d %B")),
        _ => format!(
            "{}'s nameday ({}) on {}",
            p.name,
            p.nameday_name.as_deref().unwrap_or(""),
            date.format("%-d %B")
        ),
    };
    let mut prev: Option<String> = None;
    for (i, (step, due)) in tpl.steps.0.iter().zip(step_dates).enumerate() {
        let key = format!("{}:{}:{}:{i}", p.id, tpl.kind, date);
        // Created before (even if deleted since): never again.
        let existing: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT id, deleted_at FROM tasks WHERE ext_source = 'occasion' AND ext_id = ? ORDER BY deleted_at IS NULL DESC LIMIT 1")
                .bind(&key)
                .fetch_optional(&mut *conn)
                .await?;
        if let Some((id, deleted)) = existing {
            prev = deleted.is_none().then_some(id);
            continue;
        }
        let last: Option<String> = sqlx::query_scalar(
            "SELECT MAX(position) FROM tasks WHERE project_id IS NULL AND owner_user_id = ? AND deleted_at IS NULL",
        )
        .bind(&user.id)
        .fetch_one(&mut *conn)
        .await?;
        let ts = now();
        let depends_on: Vec<String> = if step.after_previous {
            prev.iter().cloned().collect()
        } else {
            vec![]
        };
        let mut t = Task {
            id: new_id(),
            owner_user_id: Some(user.id.clone()),
            owner_group_id: None,
            assignee_user_id: None,
            project_id: None,
            title: step.title.replace("{name}", &p.name),
            notes: what.clone(),
            status: "open".into(),
            position: key_after(last.as_deref()),
            due_date: Some(due.format("%Y-%m-%d").to_string()),
            estimate_min: None,
            difficulty: None,
            importance: None,
            urgency: None,
            actual_min: 0,
            task_type_id: step.task_type_id.clone(),
            carry_count: 0,
            started_at: None,
            completed_at: None,
            completed_by: None,
            ext_source: Some("occasion".into()),
            ext_id: Some(key),
            ext_url: None,
            place_id: None,
            also_project_ids: sqlx::types::Json(vec![]),
            depends_on: sqlx::types::Json(depends_on),
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
            created_at: ts.clone(),
            updated_at: ts,
            deleted_at: None,
            rev: next_rev(conn).await?,
        };
        crate::deps::refresh(conn, &mut t).await?;
        upsert_task(conn, &t).await?;
        changes.push(Change::task(&t));
        prev = Some(t.id.clone());
    }
    Ok(())
}

pub async fn materialize_all(state: &AppState) -> anyhow::Result<()> {
    let users: Vec<User> = sqlx::query_as(
        "SELECT * FROM users WHERE deleted_at IS NULL AND id IN (SELECT owner_user_id FROM people WHERE deleted_at IS NULL)",
    )
    .fetch_all(&state.db.read)
    .await?;
    for u in users {
        if let Err(e) = materialize_user(state, &u).await {
            tracing::warn!("occasions failed for user {}: {e:#}", u.id);
        }
    }
    Ok(())
}

/// Replace the nameday calendar.
pub async fn load_namedays(
    state: &AppState,
    list: &[Nameday],
    label: &str,
) -> anyhow::Result<usize> {
    let mut tx = state.db.write.begin().await?;
    sqlx::query("DELETE FROM namedays")
        .execute(&mut *tx)
        .await?;
    let mut n = 0;
    for d in list {
        n += sqlx::query(
            "INSERT OR IGNORE INTO namedays (month, day, name, name_key) VALUES (?,?,?,?)",
        )
        .bind(d.month)
        .bind(d.day)
        .bind(&d.name)
        .bind(fold(&d.name))
        .execute(&mut *tx)
        .await?
        .rows_affected() as usize;
    }
    sqlx::query("INSERT INTO nameday_source (id, label, loaded_at, count) VALUES (1, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET label = excluded.label, loaded_at = excluded.loaded_at, count = excluded.count")
        .bind(label)
        .bind(now())
        .bind(n as i64)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let users: Vec<String> = sqlx::query_scalar("SELECT id FROM users WHERE deleted_at IS NULL")
        .fetch_all(&state.db.read)
        .await?;
    state.bus.publish([Change::namedays(users)]);
    Ok(n)
}

/// Download and load the official list.
pub async fn fetch_official(state: &AppState, url: &str) -> anyhow::Result<usize> {
    let res = crate::net::client(30, true)?
        .get(url)
        .send()
        .await
        .map_err(|e| anyhow::Error::new(e.without_url()).context("can't reach the nameday list"))?;
    if !res.status().is_success() {
        bail!("the nameday list answered {}", res.status());
    }
    let html = crate::net::text_limited(res, 8 * 1024 * 1024).await?;
    let list = parse_stat_ee(&html)
        .map_err(anyhow::Error::msg)
        .context("can't read the nameday list")?;
    load_namedays(
        state,
        &list,
        "Estonian (Statistics Estonia; P. Mäeniit, Eesti nimed)",
    )
    .await
}

static LAST_TRY: Mutex<Option<Instant>> = Mutex::new(None);

/// Background: if there's no nameday calendar yet, fetch the official one (at most every
/// 6 hours while it fails). Set `NAMEDAYS_URL=off` to never download.
pub async fn ensure_calendar(state: &AppState) {
    let Some(url) = state.config.namedays_url.clone() else {
        return;
    };
    {
        let mut last = LAST_TRY.lock().unwrap();
        if last.is_some_and(|t| t.elapsed().as_secs() < 6 * 3600) {
            return;
        }
        *last = Some(Instant::now());
    }
    let have: i64 = match sqlx::query_scalar("SELECT COUNT(*) FROM namedays")
        .fetch_one(&state.db.read)
        .await
    {
        Ok(n) => n,
        Err(_) => return,
    };
    if have > 0 {
        return;
    }
    match fetch_official(state, &url).await {
        Ok(n) => tracing::info!("loaded {n} namedays from {url}"),
        Err(e) => tracing::warn!("nameday calendar not loaded: {e:#}"),
    }
}

/// Names matching a search, best first: exact, then prefix, then anywhere in the name.
pub async fn search(
    conn: &mut SqliteConnection,
    q: &str,
    limit: i64,
) -> sqlx::Result<Vec<(String, Vec<(u32, u32)>)>> {
    let key = fold(q);
    if key.is_empty() {
        return Ok(vec![]);
    }
    let rows: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT name, month, day FROM namedays WHERE name_key LIKE '%' || ?1 || '%' ESCAPE '\\'
         ORDER BY name_key <> ?1, name_key NOT LIKE ?1 || '%' ESCAPE '\\', name_key, month, day",
    )
    .bind(
        key.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_"),
    )
    .fetch_all(conn)
    .await?;
    let mut out: Vec<(String, Vec<(u32, u32)>)> = vec![];
    let mut index: HashMap<String, usize> = HashMap::new();
    for (name, m, d) in rows {
        match index.get(&name) {
            Some(&i) => out[i].1.push((m as u32, d as u32)),
            None => {
                if out.len() as i64 >= limit {
                    continue;
                }
                index.insert(name.clone(), out.len());
                out.push((name, vec![(m as u32, d as u32)]));
            }
        }
    }
    Ok(out)
}
