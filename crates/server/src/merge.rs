//! Offline edits (D-79): an update may carry the values it was based on and when it was
//! made. The covered handlers load the record, let `domain::merge` decide per field what to
//! apply, then run as usual on what's left; the response lists the fields both sides had
//! changed. Without `base` an update applies as sent (last write wins), as before.

use chrono::{DateTime, Utc};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use streamline_domain::merge::{Side, merge_fields};
use ts_rs::TS;

use crate::error::{ApiResult, bad};

/// What an offline edit carries besides its fields.
#[derive(Debug, Default, serde::Deserialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct MergeBase {
    /// The values the edited fields had when the edit was made (API field names). Fields
    /// that changed on the server since then keep the newer change; checklists merge per
    /// item.
    #[serde(default)]
    #[ts(optional, type = "Record<string, unknown>")]
    #[schema(value_type = Option<Object>)]
    pub base: Option<Map<String, Value>>,
    /// When the edit was made (RFC 3339; defaults to now, later times count as now).
    #[serde(default)]
    #[ts(optional)]
    pub edited_at: Option<String>,
}

/// A field (or checklist item) changed on both sides since the edit's base.
#[derive(Debug, Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct Conflict {
    pub field: String,
    /// The checklist item id, for `checklist`.
    pub item: Option<String>,
    /// Whose change was kept: `server` (the edit's value was dropped) or `client` (the
    /// server's value was overwritten).
    pub kept: String,
    #[ts(type = "unknown")]
    #[schema(value_type = Object)]
    pub server: Value,
    #[ts(type = "unknown")]
    #[schema(value_type = Object)]
    pub client: Value,
}

/// A record after an update, with the conflicts its merge had (left out when none).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Merged<T> {
    #[serde(flatten)]
    pub item: T,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<Conflict>,
}

impl<T> Merged<T> {
    pub fn new(item: T, conflicts: Vec<Conflict>) -> Self {
        Self { item, conflicts }
    }
}

/// Merge an update's JSON body into `current` (the record as the API returns it, last
/// changed at `updated_at`) and parse what remains as the handler's request type.
pub fn prepare<P: DeserializeOwned, R: Serialize>(
    body: Value,
    current: Option<&R>,
    updated_at: Option<&str>,
) -> ApiResult<(P, Vec<Conflict>)> {
    merge(body, current, updated_at, false)
}

/// Like `prepare`, for a PUT that sets every field: the fields the server keeps are sent
/// back with the server's values instead of being left out.
pub fn prepare_put<P: DeserializeOwned, R: Serialize>(
    body: Value,
    current: Option<&R>,
    updated_at: Option<&str>,
) -> ApiResult<(P, Vec<Conflict>)> {
    merge(body, current, updated_at, true)
}

fn merge<P: DeserializeOwned, R: Serialize>(
    body: Value,
    current: Option<&R>,
    updated_at: Option<&str>,
    fill: bool,
) -> ApiResult<(P, Vec<Conflict>)> {
    let Value::Object(mut edit) = body else {
        return Err(bad("expected a JSON object"));
    };
    let base = edit.remove("base");
    let edited_at = edit.remove("edited_at");
    // The checklist merge compares item lists pairwise: refuse oversized ones before it
    // runs (they'd be refused by validation afterwards anyway; security review).
    let too_long = |list: Option<&Value>| {
        list.and_then(Value::as_array)
            .is_some_and(|l| l.len() > streamline_domain::checklist::MAX_ITEMS)
    };
    if too_long(edit.get("checklist")) || too_long(base.as_ref().and_then(|b| b.get("checklist"))) {
        return Err(bad("a checklist can have at most 50 items"));
    }
    let mut conflicts = Vec::new();
    match (base, current) {
        (None | Some(Value::Null), _) | (_, None) => {}
        (Some(Value::Object(base)), Some(current)) => {
            let now = Utc::now();
            let edited = match edited_at {
                None | Some(Value::Null) => now,
                Some(Value::String(s)) => DateTime::parse_from_rfc3339(&s)
                    .map_err(|_| bad("edited_at must be an RFC 3339 time"))?
                    .with_timezone(&Utc)
                    .min(now),
                Some(_) => return Err(bad("edited_at must be an RFC 3339 time")),
            };
            let server_at = updated_at
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|t| t.with_timezone(&Utc));
            let client_newer = server_at.is_none_or(|s| edited > s);
            let Value::Object(cur) = serde_json::to_value(current)? else {
                return Err(bad("record is not an object"));
            };
            let sent: Vec<String> = edit.keys().cloned().collect();
            let (mut merged, cs) = merge_fields(edit, &base, &cur, client_newer);
            if fill {
                for k in sent {
                    if !merged.contains_key(&k)
                        && let Some(v) = cur.get(&k)
                    {
                        merged.insert(k, v.clone());
                    }
                }
            }
            edit = merged;
            conflicts = cs
                .into_iter()
                .map(|c| Conflict {
                    field: c.field,
                    item: c.item,
                    kept: match c.kept {
                        Side::Server => "server",
                        Side::Client => "client",
                    }
                    .into(),
                    server: c.server,
                    client: c.client,
                })
                .collect();
        }
        (Some(_), _) => return Err(bad("base must be an object of field values")),
    }
    let parsed = serde_json::from_value(Value::Object(edit))
        .map_err(|e| bad(format!("invalid request: {e}")))?;
    Ok((parsed, conflicts))
}
