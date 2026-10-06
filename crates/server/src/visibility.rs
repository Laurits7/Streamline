//! The single place that decides who can see what (WORKPLAN A.4.4).
//! Phase 1 only has personal ownership; groups extend these functions in Phase 4.

use crate::models::User;

/// SQL condition (with one bind for the user id) selecting rows visible to a user,
/// for tables with `owner_user_id` / `owner_group_id`.
pub const OWNED_VISIBLE_SQL: &str = "owner_user_id = ?";

pub fn can_see(user: &User, owner_user_id: Option<&str>, _owner_group_id: Option<&str>) -> bool {
    owner_user_id == Some(user.id.as_str())
}

/// Users who should receive realtime events about an owned entity.
pub fn audience(owner_user_id: Option<&str>, _owner_group_id: Option<&str>) -> Vec<String> {
    owner_user_id
        .map(|u| vec![u.to_string()])
        .unwrap_or_default()
}
