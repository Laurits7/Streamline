//! The single place that decides who can see what (WORKPLAN A.4.4, Phase 4).
//! Owned entities (projects, tasks, routines, places, workflow templates) belong to a
//! user or to a group; group members see group-owned ones. Personal data (day plans,
//! focus sessions, planning state, the activity log) is always filtered by user alone.

use crate::auth::AuthUser;

/// SQL condition selecting rows visible to the user bound as `?1`, for tables with
/// `owner_user_id` / `owner_group_id`. Bind the user id as the *first* parameter.
pub const OWNED_VISIBLE_SQL: &str = "(owner_user_id = ?1 OR owner_group_id IN (SELECT group_id FROM group_members WHERE user_id = ?1))";

/// The same condition for a table referenced through an alias (e.g. `t.`).
pub fn visible_sql(alias: &str) -> String {
    format!(
        "({alias}owner_user_id = ?1 OR {alias}owner_group_id IN (SELECT group_id FROM group_members WHERE user_id = ?1))"
    )
}

pub fn can_see(user: &AuthUser, owner_user_id: Option<&str>, owner_group_id: Option<&str>) -> bool {
    owner_user_id == Some(user.id())
        || owner_group_id.is_some_and(|g| user.groups.iter().any(|x| x == g))
}

/// Who gets realtime events about an owned entity: the owning user, or `g:<group>` for
/// every member of the owning group (resolved per subscriber in the SSE stream).
pub fn audience(owner_user_id: Option<&str>, owner_group_id: Option<&str>) -> Vec<String> {
    match (owner_user_id, owner_group_id) {
        (Some(u), _) => vec![u.to_string()],
        (None, Some(g)) => vec![group_key(g)],
        _ => vec![],
    }
}

pub fn group_key(group_id: &str) -> String {
    format!("g:{group_id}")
}
