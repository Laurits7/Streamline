//! Who owns what (Phase 4): a user or a group. Tasks and routines in a project share the
//! project's owner; sharing a top-level project with a group shares its whole subtree.

use sqlx::SqliteConnection;

use crate::{
    auth::AuthUser,
    error::{ApiResult, bad},
    events::Change,
    models::{Project, Series, Task, upsert_project, upsert_series, upsert_task},
    visibility,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owner {
    pub user: Option<String>,
    pub group: Option<String>,
}

impl Owner {
    pub fn me(user: &AuthUser) -> Self {
        Self {
            user: Some(user.id().into()),
            group: None,
        }
    }
    pub fn of_project(p: &Project) -> Self {
        Self {
            user: p.owner_user_id.clone(),
            group: p.owner_group_id.clone(),
        }
    }
    pub fn audience(&self) -> Vec<String> {
        visibility::audience(self.user.as_deref(), self.group.as_deref())
    }
}

/// `Some(group)` → that group (you must be a member); `None` → you.
pub fn chosen(user: &AuthUser, group: Option<&str>) -> ApiResult<Owner> {
    match group {
        Some(g) if user.groups.iter().any(|x| x == g) => Ok(Owner {
            user: None,
            group: Some(g.into()),
        }),
        Some(_) => Err(bad("you're not a member of that group")),
        None => Ok(Owner::me(user)),
    }
}

pub async fn project_owner(
    conn: &mut SqliteConnection,
    project_id: &str,
) -> sqlx::Result<Option<Owner>> {
    let p: Option<Project> = sqlx::query_as("SELECT * FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_optional(conn)
        .await?;
    Ok(p.map(|p| Owner::of_project(&p)))
}

/// Tell people who could see something but no longer can that it's gone for them.
pub fn gone_for(change: &Change, old: &[String], ts: &str) -> Option<Change> {
    let lost: Vec<String> = old
        .iter()
        .filter(|a| !change.audience.contains(a))
        .cloned()
        .collect();
    if lost.is_empty() {
        return None;
    }
    let mut data = change.data.clone();
    data["deleted_at"] = serde_json::Value::String(ts.into());
    Some(Change {
        rev: change.rev,
        kind: change.kind,
        data,
        audience: lost,
    })
}

/// Give a project, its subprojects, and their tasks and routines a new owner.
pub async fn reown_subtree(
    conn: &mut SqliteConnection,
    root: &str,
    owner: &Owner,
    rev: i64,
    ts: &str,
    changes: &mut Vec<Change>,
) -> sqlx::Result<()> {
    let mut ids = vec![root.to_string()];
    let mut i = 0;
    while i < ids.len() {
        let kids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM projects WHERE parent_id = ? AND deleted_at IS NULL",
        )
        .bind(&ids[i])
        .fetch_all(&mut *conn)
        .await?;
        ids.extend(kids);
        i += 1;
    }
    for pid in &ids {
        let mut p: Project = sqlx::query_as("SELECT * FROM projects WHERE id = ?")
            .bind(pid)
            .fetch_one(&mut *conn)
            .await?;
        let old = Owner::of_project(&p).audience();
        p.owner_user_id = owner.user.clone();
        p.owner_group_id = owner.group.clone();
        p.updated_at = ts.into();
        p.rev = rev;
        upsert_project(conn, &p).await?;
        let c = Change::project(&p);
        changes.extend(gone_for(&c, &old, ts));
        changes.push(c);
        let tasks: Vec<Task> =
            sqlx::query_as("SELECT * FROM tasks WHERE project_id = ? AND deleted_at IS NULL")
                .bind(pid)
                .fetch_all(&mut *conn)
                .await?;
        for mut t in tasks {
            let old = visibility::audience(t.owner_user_id.as_deref(), t.owner_group_id.as_deref());
            t.owner_user_id = owner.user.clone();
            t.owner_group_id = owner.group.clone();
            t.updated_at = ts.into();
            t.rev = rev;
            upsert_task(conn, &t).await?;
            let c = Change::task(&t);
            changes.extend(gone_for(&c, &old, ts));
            changes.push(c);
        }
        let series: Vec<Series> =
            sqlx::query_as("SELECT * FROM series WHERE project_id = ? AND deleted_at IS NULL")
                .bind(pid)
                .fetch_all(&mut *conn)
                .await?;
        for mut s in series {
            let old = visibility::audience(s.owner_user_id.as_deref(), s.owner_group_id.as_deref());
            s.owner_user_id = owner.user.clone();
            s.owner_group_id = owner.group.clone();
            s.updated_at = ts.into();
            s.rev = rev;
            upsert_series(conn, &s).await?;
            let c = Change::series(&s);
            changes.extend(gone_for(&c, &old, ts));
            changes.push(c);
        }
    }
    Ok(())
}
