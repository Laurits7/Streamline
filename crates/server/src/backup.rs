//! Backups (Phase 6.3, D-63): a consistent copy of the database (`VACUUM INTO`) in
//! `data/backups/` on a schedule, keeping the newest few. Admins can list, make and
//! download them. Restore = stop, put a copy in place as `streamline.db`, start.

use std::path::PathBuf;

use anyhow::Context;
use serde::Serialize;

use crate::AppState;

#[derive(Serialize, utoipa::ToSchema, ts_rs::TS)]
#[ts(export)]
pub struct Backup {
    pub name: String,
    #[ts(type = "number")]
    pub bytes: u64,
    /// RFC 3339 (from the file name).
    pub created_at: String,
}

pub fn dir(state: &AppState) -> PathBuf {
    state.config.data_dir.join("backups")
}

/// `streamline-YYYYMMDD-HHMMSS.db` only (also guards downloads against path tricks).
pub fn valid_name(name: &str) -> bool {
    let Some(stamp) = name
        .strip_prefix("streamline-")
        .and_then(|r| r.strip_suffix(".db"))
    else {
        return false;
    };
    stamp.len() == 15
        && stamp.as_bytes()[8] == b'-'
        && stamp
            .chars()
            .enumerate()
            .all(|(i, c)| i == 8 || c.is_ascii_digit())
}

pub fn list(state: &AppState) -> anyhow::Result<Vec<Backup>> {
    let mut out = vec![];
    let Ok(entries) = std::fs::read_dir(dir(state)) else {
        return Ok(out);
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !valid_name(&name) {
            continue;
        }
        let s = &name[11..26];
        let created_at = format!(
            "{}-{}-{}T{}:{}:{}Z",
            &s[0..4],
            &s[4..6],
            &s[6..8],
            &s[9..11],
            &s[11..13],
            &s[13..15]
        );
        out.push(Backup {
            bytes: e.metadata().map(|m| m.len()).unwrap_or(0),
            name,
            created_at,
        });
    }
    out.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(out)
}

/// Make a backup now and drop the oldest beyond `BACKUP_KEEP`.
pub async fn run(state: &AppState) -> anyhow::Result<Backup> {
    let d = dir(state);
    std::fs::create_dir_all(&d).with_context(|| format!("creating {}", d.display()))?;
    let name = format!(
        "streamline-{}.db",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    );
    let path = d.join(&name);
    // On the writer connection, so the copy can't race a write.
    sqlx::query("VACUUM INTO ?")
        .bind(path.to_string_lossy().to_string())
        .execute(&state.db.write)
        .await
        .context("backing up the database")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    let all = list(state)?;
    for old in all.iter().skip(state.config.backup_keep.max(1)) {
        let _ = std::fs::remove_file(d.join(&old.name));
    }
    tracing::info!("backup written: {name}");
    list(state)?
        .into_iter()
        .find(|b| b.name == name)
        .context("backup vanished")
}

/// Background: back up when the newest backup is older than `BACKUP_HOURS` (0 = never).
pub fn spawn(state: AppState) {
    if state.config.backup_hours == 0 {
        return;
    }
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(15 * 60));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let newest = list(&state).ok().and_then(|l| l.into_iter().next());
            let due = newest.is_none_or(|b| {
                chrono::DateTime::parse_from_rfc3339(&b.created_at)
                    .map(|t| {
                        chrono::Utc::now() - t.to_utc()
                            >= chrono::Duration::hours(state.config.backup_hours as i64)
                    })
                    .unwrap_or(true)
            });
            if due {
                let s = state.clone();
                crate::util::guarded("backup", async move {
                    if let Err(e) = run(&s).await {
                        tracing::warn!("backup failed: {e:#}");
                    }
                })
                .await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn names() {
        assert!(super::valid_name("streamline-20261006-221500.db"));
        assert!(!super::valid_name("streamline-20261006-221500.db-wal"));
        assert!(!super::valid_name("../streamline.db"));
        assert!(!super::valid_name("streamline-2026100x-221500.db"));
    }
}
