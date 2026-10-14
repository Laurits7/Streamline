//! Merging an edit made offline into what the server has now (D-79). The client sends the
//! fields it changed, the values it based them on (`base`) and when it made the edit. Per
//! field: if the server still has the base value, or already has the same value, the edit
//! applies; if both sides changed it, the later change wins. Checklists merge per item, by
//! item id. A deletion beats an edit, on items as on whole records (the server refuses
//! edits to deleted records before getting here).
//!
//! The server's side has no per-field times: the record's last change stands in for each
//! field's, which can favour the server when another field changed later.

use serde::Serialize;
use serde_json::{Map, Value};

use crate::checklist::Item;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Server,
    Client,
}

/// A field (or checklist item) both sides changed, and which change was kept.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Conflict {
    pub field: String,
    /// The checklist item, for `checklist`.
    pub item: Option<String>,
    pub kept: Side,
    pub server: Value,
    pub client: Value,
}

/// Merge the fields of an edit into the server's current record (both as JSON objects with
/// the API's field names). Returns the edit to apply (fields the server keeps are removed,
/// a checklist is replaced by the merged list) and the conflicts. Fields without a base
/// value, or that the record doesn't have, apply as sent (last write wins).
pub fn merge_fields(
    edit: Map<String, Value>,
    base: &Map<String, Value>,
    current: &Map<String, Value>,
    client_newer: bool,
) -> (Map<String, Value>, Vec<Conflict>) {
    let mut out = Map::new();
    let mut conflicts = Vec::new();
    for (k, v) in edit {
        let (Some(b), Some(s)) = (base.get(&k), current.get(&k)) else {
            out.insert(k, v);
            continue;
        };
        if k == "checklist" {
            let parse = |v: &Value| serde_json::from_value::<Vec<Item>>(v.clone()).ok();
            if let (Some(b), Some(s), Some(c)) = (parse(b), parse(s), parse(&v)) {
                let (merged, cs) = merge_checklist(&b, &s, &c, client_newer);
                conflicts.extend(cs);
                out.insert(k, serde_json::to_value(merged).unwrap_or(v));
                continue;
            }
        }
        if s == b || *s == v {
            out.insert(k, v);
        } else if v == *b {
            // Unchanged on the client: the server's change stands.
        } else {
            let kept = if client_newer {
                Side::Client
            } else {
                Side::Server
            };
            conflicts.push(Conflict {
                field: k.clone(),
                item: None,
                kept,
                server: s.clone(),
                client: v.clone(),
            });
            if kept == Side::Client {
                out.insert(k, v);
            }
        }
    }
    (out, conflicts)
}

/// Three-way merge of one value: the side that changed it wins; both changed → the newer.
/// Returns the winning side and whether both had changed it. Equal values count as the
/// server's, so a tick both sides made keeps the server's record of it.
fn pick<T: PartialEq>(b: Option<&T>, s: &T, c: &T, client_newer: bool) -> (Side, bool) {
    if s == c || Some(c) == b {
        (Side::Server, false)
    } else if Some(s) == b {
        (Side::Client, false)
    } else {
        (
            if client_newer {
                Side::Client
            } else {
                Side::Server
            },
            true,
        )
    }
}

/// Merge checklists by item id: additions from both sides are kept, an item deleted on
/// either side stays deleted, and the text and tick of an item both sides changed come
/// from the newer edit (a tick carries its `done_at`/`done_by` along). The client's order
/// is kept; items only the server added follow the item before them on the server.
pub fn merge_checklist(
    base: &[Item],
    server: &[Item],
    client: &[Item],
    client_newer: bool,
) -> (Vec<Item>, Vec<Conflict>) {
    let find = |l: &[Item], id: &str| l.iter().find(|i| i.id == id).cloned();
    let mut out: Vec<Item> = Vec::new();
    let mut conflicts = Vec::new();
    for c in client {
        let b = find(base, &c.id);
        let Some(s) = find(server, &c.id) else {
            // Deleted on the server: stays deleted. New on the client: added.
            if b.is_none() {
                out.push(c.clone());
            }
            continue;
        };
        let side = |p: Side| if p == Side::Client { c } else { &s };
        let (text, t_conflict) = pick(b.as_ref().map(|b| &b.text), &s.text, &c.text, client_newer);
        let (tick, d_conflict) = pick(b.as_ref().map(|b| &b.done), &s.done, &c.done, client_newer);
        let tick = side(tick);
        if t_conflict || d_conflict {
            let view = |i: &Item| serde_json::json!({"text": i.text, "done": i.done});
            conflicts.push(Conflict {
                field: "checklist".into(),
                item: Some(c.id.clone()),
                kept: if client_newer {
                    Side::Client
                } else {
                    Side::Server
                },
                server: view(&s),
                client: view(c),
            });
        }
        out.push(Item {
            id: c.id.clone(),
            text: side(text).text.clone(),
            done: tick.done,
            done_at: tick.done_at.clone(),
            done_by: tick.done_by.clone(),
        });
    }
    // Items only the server has: added there (kept), or deleted on the client (dropped).
    for (i, s) in server.iter().enumerate() {
        if client.iter().any(|c| c.id == s.id) || base.iter().any(|b| b.id == s.id) {
            continue;
        }
        let at = server[..i]
            .iter()
            .rev()
            .find_map(|p| out.iter().position(|o| o.id == p.id))
            .map_or(0, |p| p + 1);
        out.insert(at, s.clone());
    }
    (out, conflicts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn fields_apply_when_the_server_kept_the_base() {
        let current = obj(json!({"title": "Laundry", "notes": "", "estimate_min": 30}));
        let base = obj(json!({"title": "Laundry", "estimate_min": 30}));
        let edit = obj(json!({"title": "Do laundry", "estimate_min": 45, "status": "done"}));
        let (out, cs) = merge_fields(edit.clone(), &base, &current, false);
        assert_eq!(out, edit);
        assert!(cs.is_empty());
    }

    #[test]
    fn both_changed_newer_wins() {
        let current =
            obj(json!({"title": "Laundry (server)", "notes": "server", "estimate_min": 60}));
        let base = obj(json!({"title": "Laundry", "notes": "", "estimate_min": 30}));
        // Notes changed on both sides to the same value: no conflict.
        let edit = obj(json!({"title": "Laundry (client)", "notes": "server", "estimate_min": 30}));
        let (out, cs) = merge_fields(edit.clone(), &base, &current, false);
        // The client sent the base estimate back unchanged: the server's 60 stays.
        assert_eq!(out, obj(json!({"notes": "server"})));
        assert_eq!(cs.len(), 1);
        assert_eq!((cs[0].field.as_str(), cs[0].kept), ("title", Side::Server));
        assert_eq!(cs[0].server, json!("Laundry (server)"));
        // The client is newer: its changed values win.
        let (out, cs) = merge_fields(edit.clone(), &base, &current, true);
        assert_eq!(
            out,
            obj(json!({"title": "Laundry (client)", "notes": "server"}))
        );
        assert_eq!((cs.len(), cs[0].kept), (1, Side::Client));
    }

    #[test]
    fn fields_without_base_or_unknown_apply() {
        let current = obj(json!({"title": "A"}));
        let base = obj(json!({"title": "B", "waiting": true}));
        let edit = obj(json!({"in_progress": true, "waiting": false, "title": "C"}));
        let (out, cs) = merge_fields(edit, &base, &current, false);
        assert_eq!(out, obj(json!({"in_progress": true, "waiting": false})));
        assert_eq!(cs.len(), 1);
        let (out, _) = merge_fields(obj(json!({"title": "C"})), &Map::new(), &current, false);
        assert_eq!(out, obj(json!({"title": "C"})));
    }

    #[test]
    fn nulls_are_values() {
        let current = obj(json!({"due_date": "2026-10-20"}));
        let base = obj(json!({"due_date": null}));
        let (out, cs) = merge_fields(
            obj(json!({"due_date": "2026-10-21"})),
            &base,
            &current,
            false,
        );
        assert!(out.is_empty());
        assert_eq!(cs[0].server, json!("2026-10-20"));
        let base = obj(json!({"due_date": "2026-10-20"}));
        let (out, cs) = merge_fields(obj(json!({"due_date": null})), &base, &current, false);
        assert_eq!(out, obj(json!({"due_date": null})));
        assert!(cs.is_empty());
    }

    fn it(id: &str, text: &str, done: bool) -> Item {
        Item {
            id: id.into(),
            text: text.into(),
            done,
            done_at: done.then(|| format!("t-{id}")),
            done_by: None,
        }
    }
    fn ids(l: &[Item]) -> Vec<(&str, &str, bool)> {
        l.iter()
            .map(|i| (i.id.as_str(), i.text.as_str(), i.done))
            .collect()
    }

    #[test]
    fn checklist_keeps_ticks_and_additions_from_both_sides() {
        let base = vec![
            it("a", "Sort", false),
            it("b", "Wash", false),
            it("c", "Fold", false),
        ];
        // Server: ticked Sort, added Iron after Wash.
        let server = vec![
            it("a", "Sort", true),
            it("b", "Wash", false),
            it("i", "Iron", false),
            it("c", "Fold", false),
        ];
        // Client (offline): ticked Wash, renamed Fold, added Hang at the end.
        let client = vec![
            it("a", "Sort", false),
            it("b", "Wash", true),
            it("c", "Fold neatly", false),
            it("h", "Hang", false),
        ];
        let (out, cs) = merge_checklist(&base, &server, &client, false);
        assert_eq!(
            ids(&out),
            vec![
                ("a", "Sort", true),
                ("b", "Wash", true),
                ("i", "Iron", false),
                ("c", "Fold neatly", false),
                ("h", "Hang", false)
            ]
        );
        assert!(cs.is_empty());
        // Each tick keeps its own record.
        assert_eq!(out[0].done_at.as_deref(), Some("t-a"));
        assert_eq!(out[1].done_at.as_deref(), Some("t-b"));
    }

    #[test]
    fn checklist_deletes_beat_edits() {
        let base = vec![it("a", "Sort", false), it("b", "Wash", false)];
        // Server deleted Sort; client ticked it. Client deleted Wash; server renamed it.
        let server = vec![it("b", "Wash at 40°", false)];
        let client = vec![it("a", "Sort", true)];
        let (out, _) = merge_checklist(&base, &server, &client, true);
        assert!(out.is_empty());
    }

    #[test]
    fn checklist_item_changed_on_both_sides() {
        let base = vec![it("a", "Sort", false)];
        let server = vec![it("a", "Sort whites", true)];
        let client = vec![it("a", "Sort colours", false)];
        let (out, cs) = merge_checklist(&base, &server, &client, false);
        assert_eq!(ids(&out), vec![("a", "Sort whites", true)]);
        assert_eq!(cs.len(), 1);
        assert_eq!(
            (cs[0].item.as_deref(), cs[0].kept),
            (Some("a"), Side::Server)
        );
        let (out, cs) = merge_checklist(&base, &server, &client, true);
        // Only the text conflicts; the tick was changed on the server alone and stays.
        assert_eq!(ids(&out), vec![("a", "Sort colours", true)]);
        assert_eq!(out[0].done_at.as_deref(), Some("t-a"));
        assert_eq!(cs[0].kept, Side::Client);
        // Ticked on the server, unticked on the (newer) client.
        let server = vec![it("a", "Sort", true)];
        let base = vec![it("a", "Sort", true)];
        let (out, _) = merge_checklist(&base, &server, &[it("a", "Sort", false)], false);
        assert_eq!(ids(&out), vec![("a", "Sort", false)]);
        assert_eq!(out[0].done_at, None);
    }

    #[test]
    fn checklist_merges_through_fields() {
        let current = obj(json!({"checklist": [{"id": "a", "text": "Sort", "done": true}]}));
        let base = obj(json!({"checklist": [{"id": "a", "text": "Sort", "done": false}]}));
        let edit = obj(
            json!({"checklist": [{"id": "a", "text": "Sort", "done": false}, {"id": "b", "text": "Wash", "done": false}]}),
        );
        let (out, cs) = merge_fields(edit, &base, &current, true);
        assert!(cs.is_empty());
        assert_eq!(
            out["checklist"],
            json!([{"id": "a", "text": "Sort", "done": true}, {"id": "b", "text": "Wash", "done": false}])
        );
    }
}
