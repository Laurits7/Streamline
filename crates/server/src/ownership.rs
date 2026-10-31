//! Who owns what (Phase 4): a user or a group. Tasks and routines in a project share the
//! project's owner, and so do multi-step chores (D-115); sharing a top-level project with a
//! group shares its whole subtree.

use sqlx::SqliteConnection;

use crate::{
    auth::AuthUser,
    db::next_rev,
    error::{ApiResult, bad},
    events::Change,
    models::{
        Project, Series, Task, WorkflowTemplate, upsert_project, upsert_series, upsert_task,
        upsert_workflow,
    },
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

/// `Some(group)` → that group (you must be a member, and it must be one of this server's,
/// not mirrored from another; Phase 8); `None` → you.
pub async fn chosen(
    state: &crate::AppState,
    user: &AuthUser,
    group: Option<&str>,
) -> ApiResult<Owner> {
    if let Some(g) = group {
        let mirrored: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM groups WHERE id = ? AND home IS NOT NULL)",
        )
        .bind(g)
        .fetch_one(&state.db.read)
        .await?;
        if mirrored {
            return Err(bad(
                "that group is mirrored from another server; share things with it there",
            ));
        }
    }
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

/// Everyone in a group, people on other servers too (they can be assigned things; D-116).
pub async fn members(conn: &mut SqliteConnection, group: &str) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT m.user_id FROM group_members m JOIN users u ON u.id = m.user_id
          WHERE m.group_id = ? AND u.deleted_at IS NULL ORDER BY m.created_at, m.user_id",
    )
    .bind(group)
    .fetch_all(conn)
    .await
}

/// Check who something owned by `group` is assigned to (D-116): only shared things can
/// be, and only to someone in the group.
pub async fn check_assignee(
    conn: &mut SqliteConnection,
    group: Option<&str>,
    assignee: Option<&str>,
) -> ApiResult<()> {
    let Some(a) = assignee else { return Ok(()) };
    let Some(g) = group else {
        return Err(bad("only things shared with a group can be assigned"));
    };
    if !members(conn, g).await?.iter().any(|m| m == a) {
        return Err(bad("only people in the group can be assigned"));
    }
    Ok(())
}

/// Check a rotation (D-116): people in the group, each once, in the order given.
pub async fn check_rotation(
    conn: &mut SqliteConnection,
    group: Option<&str>,
    rotation: Vec<String>,
) -> ApiResult<Vec<String>> {
    if rotation.is_empty() {
        return Ok(rotation);
    }
    let Some(g) = group else {
        return Err(bad("only things shared with a group can take turns"));
    };
    let members = members(conn, g).await?;
    if rotation.len() > 50 || rotation.iter().any(|u| !members.contains(u)) {
        return Err(bad("only people in the group can take turns"));
    }
    Ok(streamline_domain::assign::clean_rotation(
        &rotation, &members,
    ))
}

/// What's left of an assignment under a (new) owning group: only people in it.
async fn kept(
    conn: &mut SqliteConnection,
    group: Option<&str>,
    assignee: Option<String>,
    rotation: Vec<String>,
) -> sqlx::Result<(Option<String>, Vec<String>)> {
    let Some(g) = group else {
        return Ok((None, vec![]));
    };
    if assignee.is_none() && rotation.is_empty() {
        return Ok((None, rotation));
    }
    let members = members(conn, g).await?;
    Ok((
        assignee.filter(|a| members.contains(a)),
        streamline_domain::assign::clean_rotation(&rotation, &members),
    ))
}

/// The assignee a task keeps when it gets a new owner (nobody, unless they're in the group).
pub async fn kept_assignee(
    conn: &mut SqliteConnection,
    group: Option<&str>,
    assignee: Option<String>,
) -> sqlx::Result<Option<String>> {
    Ok(kept(conn, group, assignee, vec![]).await?.0)
}

/// Someone left a group (or was removed): their open tasks in it go back to anyone, and
/// they're no longer the default or in the turns of its routines and multi-step chores.
pub async fn unassign(
    conn: &mut SqliteConnection,
    group: &str,
    user_id: &str,
) -> sqlx::Result<Vec<Change>> {
    let mut changes = vec![];
    let tasks: Vec<Task> = sqlx::query_as(
        "SELECT * FROM tasks WHERE owner_group_id = ? AND assignee_user_id = ? AND status = 'open' AND deleted_at IS NULL",
    )
    .bind(group)
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await?;
    let series: Vec<Series> = sqlx::query_as(
        "SELECT s.* FROM series s WHERE s.owner_group_id = ?1 AND s.deleted_at IS NULL
           AND (s.assignee_user_id = ?2 OR EXISTS (SELECT 1 FROM json_each(s.rotation_user_ids) j WHERE j.value = ?2))",
    )
    .bind(group)
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await?;
    let workflows: Vec<WorkflowTemplate> = sqlx::query_as(
        "SELECT w.* FROM workflow_templates w WHERE w.owner_group_id = ?1 AND w.deleted_at IS NULL
           AND (w.assignee_user_id = ?2 OR EXISTS (SELECT 1 FROM json_each(w.rotation_user_ids) j WHERE j.value = ?2))",
    )
    .bind(group)
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await?;
    if tasks.is_empty() && series.is_empty() && workflows.is_empty() {
        return Ok(changes);
    }
    let rev = next_rev(conn).await?;
    let ts = crate::util::now();
    for mut t in tasks {
        t.assignee_user_id = None;
        t.updated_at = ts.clone();
        t.rev = rev;
        upsert_task(conn, &t).await?;
        changes.push(Change::task(&t));
    }
    let not_them = |u: &Option<String>| u.clone().filter(|u| u != user_id);
    for mut s in series {
        s.assignee_user_id = not_them(&s.assignee_user_id);
        s.rotation_user_ids.0.retain(|u| u != user_id);
        s.updated_at = ts.clone();
        s.rev = rev;
        upsert_series(conn, &s).await?;
        changes.push(Change::series(&s));
    }
    for mut w in workflows {
        w.assignee_user_id = not_them(&w.assignee_user_id);
        w.rotation_user_ids.0.retain(|u| u != user_id);
        w.updated_at = ts.clone();
        w.rev = rev;
        upsert_workflow(conn, &w).await?;
        changes.push(Change::workflow(&w));
    }
    Ok(changes)
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

/// Give a project, its subprojects, and their tasks, routines and multi-step chores a new
/// owner.
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
            t.assignee_user_id =
                kept_assignee(conn, owner.group.as_deref(), t.assignee_user_id.take()).await?;
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
            (s.assignee_user_id, s.rotation_user_ids.0) = kept(
                conn,
                owner.group.as_deref(),
                s.assignee_user_id.take(),
                std::mem::take(&mut s.rotation_user_ids.0),
            )
            .await?;
            s.updated_at = ts.into();
            s.rev = rev;
            upsert_series(conn, &s).await?;
            let c = Change::series(&s);
            changes.extend(gone_for(&c, &old, ts));
            changes.push(c);
        }
        let workflows: Vec<WorkflowTemplate> = sqlx::query_as(
            "SELECT * FROM workflow_templates WHERE project_id = ? AND deleted_at IS NULL",
        )
        .bind(pid)
        .fetch_all(&mut *conn)
        .await?;
        for mut w in workflows {
            let old = visibility::audience(w.owner_user_id.as_deref(), w.owner_group_id.as_deref());
            w.owner_user_id = owner.user.clone();
            w.owner_group_id = owner.group.clone();
            (w.assignee_user_id, w.rotation_user_ids.0) = kept(
                conn,
                owner.group.as_deref(),
                w.assignee_user_id.take(),
                std::mem::take(&mut w.rotation_user_ids.0),
            )
            .await?;
            w.updated_at = ts.into();
            w.rev = rev;
            upsert_workflow(conn, &w).await?;
            let c = Change::workflow(&w);
            changes.extend(gone_for(&c, &old, ts));
            changes.push(c);
        }
    }
    Ok(())
}
