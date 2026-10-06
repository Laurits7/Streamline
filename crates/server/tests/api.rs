//! End-to-end API tests against a real (temporary) SQLite database.

use std::net::SocketAddr;

use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use streamline::{AppState, config::Config};
use tower::ServiceExt;

struct T {
    app: Router,
    state: AppState,
}

async fn setup() -> T {
    let dir = std::env::temp_dir().join(format!("streamline-test-{}", ulid::Ulid::new()));
    let (app, state) = streamline::build(Config::for_tests(dir)).await.unwrap();
    T { app, state }
}

impl T {
    async fn req(
        &self,
        method: &str,
        path: &str,
        auth: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value, Option<String>) {
        let mut b = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "localhost:3000");
        if let Some(a) = auth {
            b = if a.starts_with("slt_") {
                b.header(header::AUTHORIZATION, format!("Bearer {a}"))
            } else {
                b.header(header::COOKIE, format!("sl_session={a}"))
            };
        }
        let body = match body {
            Some(v) => {
                b = b.header(header::CONTENT_TYPE, "application/json");
                Body::from(v.to_string())
            }
            None => Body::empty(),
        };
        let mut req = b.body(body).unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 9999))));
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let cookie = res
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|c| c.split(';').next())
            .and_then(|kv| kv.strip_prefix("sl_session="))
            .map(String::from);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, json, cookie)
    }

    /// First-run setup; returns the admin's session token.
    async fn admin(&self) -> String {
        let (s, _, cookie) = self
            .req(
                "POST",
                "/api/v1/setup",
                None,
                Some(json!({"username": "admin", "password": "password123"})),
            )
            .await;
        assert_eq!(s, StatusCode::OK);
        cookie.unwrap()
    }

    async fn user(&self, admin: &str, name: &str) -> String {
        let (s, _, _) = self
            .req(
                "POST",
                "/api/v1/users",
                Some(admin),
                Some(json!({"username": name, "password": "password123"})),
            )
            .await;
        assert_eq!(s, StatusCode::OK);
        let (s, _, cookie) = self
            .req(
                "POST",
                "/api/v1/auth/login",
                None,
                Some(json!({"username": name, "password": "password123"})),
            )
            .await;
        assert_eq!(s, StatusCode::OK);
        cookie.unwrap()
    }
}

#[tokio::test]
async fn setup_login_and_me() {
    let t = setup().await;
    let (_, v, _) = t.req("GET", "/api/v1/setup", None, None).await;
    assert_eq!(v["needs_setup"], true);
    let admin = t.admin().await;
    let (s, v, _) = t.req("GET", "/api/v1/me", Some(&admin), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["username"], "admin");
    assert_eq!(v["is_admin"], true);
    // Setup can only run once.
    let (s, _, _) = t
        .req(
            "POST",
            "/api/v1/setup",
            None,
            Some(json!({"username": "x", "password": "password123"})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // Wrong password.
    let (s, _, _) = t
        .req(
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({"username": "admin", "password": "nope-nope"})),
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    // Unauthenticated.
    let (s, _, _) = t.req("GET", "/api/v1/me", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    // Logout invalidates the session.
    let (s, _, _) = t
        .req("POST", "/api/v1/auth/logout", Some(&admin), None)
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _, _) = t.req("GET", "/api/v1/me", Some(&admin), None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn api_tokens() {
    let t = setup().await;
    let admin = t.admin().await;
    let (_, v, _) = t
        .req(
            "POST",
            "/api/v1/tokens",
            Some(&admin),
            Some(json!({"name": "laptop"})),
        )
        .await;
    let token = v["token"].as_str().unwrap().to_string();
    let id = v["info"]["id"].as_str().unwrap().to_string();
    let (s, v, _) = t.req("GET", "/api/v1/me", Some(&token), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["username"], "admin");
    t.req(
        "DELETE",
        &format!("/api/v1/tokens/{id}"),
        Some(&admin),
        None,
    )
    .await;
    let (s, _, _) = t.req("GET", "/api/v1/me", Some(&token), None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn cross_origin_writes_are_rejected() {
    let t = setup().await;
    let admin = t.admin().await;
    let mut req = Request::builder()
        .method("POST")
        .uri("/api/v1/tasks")
        .header(header::HOST, "localhost:3000")
        .header(header::ORIGIN, "http://evil.example")
        .header(header::COOKIE, format!("sl_session={admin}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"title": "x"}).to_string()))
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 1))));
    let res = t.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn projects_tasks_and_privacy() {
    let t = setup().await;
    let admin = t.admin().await;
    let bob = t.user(&admin, "bob").await;

    let (s, p, _) = t
        .req(
            "POST",
            "/api/v1/projects",
            Some(&admin),
            Some(json!({"name": "House"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let pid = p["id"].as_str().unwrap();
    let (s, task, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&admin),
            Some(json!({"title": "Fix bike", "project_id": pid, "difficulty": 2})),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let tid = task["id"].as_str().unwrap();
    assert_eq!(task["task_type_id"], "tt_carry_on");

    // Validation.
    let (s, _, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&admin),
            Some(json!({"title": "x", "difficulty": 9})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&admin),
            Some(json!({"title": "  "})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Complete, records who.
    let (_, v, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/tasks/{tid}"),
            Some(&admin),
            Some(json!({"status": "done"})),
        )
        .await;
    assert_eq!(v["status"], "done");
    assert!(v["completed_by"].is_string());
    // Clearing a field with null.
    let (_, v, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/tasks/{tid}"),
            Some(&admin),
            Some(json!({"difficulty": null})),
        )
        .await;
    assert!(v["difficulty"].is_null());

    // Bob sees nothing of admin's and cannot touch it.
    let (_, v, _) = t.req("GET", "/api/v1/sync", Some(&bob), None).await;
    assert_eq!(v["projects"].as_array().unwrap().len(), 0);
    assert_eq!(v["tasks"].as_array().unwrap().len(), 0);
    let (s, _, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/tasks/{tid}"),
            Some(&bob),
            Some(json!({"title": "hacked"})),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&bob),
            Some(json!({"title": "x", "project_id": pid})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Delta sync after deleting the project returns tombstones.
    let (_, v, _) = t.req("GET", "/api/v1/sync", Some(&admin), None).await;
    let rev = v["rev"].as_i64().unwrap();
    t.req(
        "DELETE",
        &format!("/api/v1/projects/{pid}"),
        Some(&admin),
        None,
    )
    .await;
    let (_, v, _) = t
        .req(
            "GET",
            &format!("/api/v1/sync?since={rev}"),
            Some(&admin),
            None,
        )
        .await;
    assert!(v["projects"][0]["deleted_at"].is_string());
    assert!(v["tasks"][0]["deleted_at"].is_string());
}

#[tokio::test]
async fn client_ids_are_idempotent() {
    let t = setup().await;
    let admin = t.admin().await;
    let id = ulid::Ulid::new().to_string();
    let body = json!({"id": id, "title": "Once"});
    let (s1, _, _) = t
        .req("POST", "/api/v1/tasks", Some(&admin), Some(body.clone()))
        .await;
    let (s2, _, _) = t
        .req("POST", "/api/v1/tasks", Some(&admin), Some(body))
        .await;
    assert_eq!((s1, s2), (StatusCode::OK, StatusCode::OK));
    let (_, v, _) = t.req("GET", "/api/v1/tasks", Some(&admin), None).await;
    assert_eq!(v.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn day_plan_and_rollover() {
    let t = setup().await;
    let admin = t.admin().await;
    let (_, v, _) = t.req("GET", "/api/v1/today", Some(&admin), None).await;
    let today = v["date"].as_str().unwrap().to_string();

    let (_, carry, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&admin),
            Some(json!({"title": "Call dentist", "day": today})),
        )
        .await;
    let (_, expire, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&admin),
            Some(json!({"title": "Vitamins", "day": today, "task_type_id": "tt_expires"})),
        )
        .await;
    let (_, day, _) = t
        .req("GET", &format!("/api/v1/days/{today}"), Some(&admin), None)
        .await;
    assert_eq!(day["entries"].as_array().unwrap().len(), 2);

    // Pretend both were planned 2 days ago and the rollover hasn't run since.
    sqlx::query("UPDATE day_entries SET date = date(?, '-2 day')")
        .bind(&today)
        .execute(&t.state.db.write)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET last_rollover_date = NULL")
        .execute(&t.state.db.write)
        .await
        .unwrap();

    let (_, day, _) = t
        .req("GET", &format!("/api/v1/days/{today}"), Some(&admin), None)
        .await;
    let entries = day["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1, "only the carry-on task moves to today");
    assert_eq!(entries[0]["task_id"], carry["id"]);
    let tasks = day["tasks"].as_array().unwrap();
    let c = tasks.iter().find(|x| x["id"] == carry["id"]).unwrap();
    assert_eq!(c["carry_count"], 2);
    let (_, e, _) = t
        .req(
            "GET",
            &format!("/api/v1/tasks/{}", expire["id"].as_str().unwrap()),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(e["status"], "missed");

    // Running again is a no-op.
    sqlx::query("UPDATE users SET last_rollover_date = NULL")
        .execute(&t.state.db.write)
        .await
        .unwrap();
    let (_, day2, _) = t
        .req("GET", &format!("/api/v1/days/{today}"), Some(&admin), None)
        .await;
    assert_eq!(day2["entries"], day["entries"]);

    // Planning the task on another day moves its single entry (snooze).
    let tomorrow = (chrono::NaiveDate::parse_from_str(&today, "%Y-%m-%d").unwrap()
        + chrono::Duration::days(1))
    .format("%Y-%m-%d")
    .to_string();
    let entry_id = entries[0]["id"].as_str().unwrap();
    let (s, v, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/day-entries/{entry_id}"),
            Some(&admin),
            Some(json!({"date": tomorrow, "start_time": "09:30"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["start_time"], "09:30");
    let (_, day, _) = t
        .req("GET", &format!("/api/v1/days/{today}"), Some(&admin), None)
        .await;
    assert_eq!(day["entries"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn subprojects() {
    let t = setup().await;
    let admin = t.admin().await;
    let mk = |name: &'static str, parent: Option<String>| {
        let admin = admin.clone();
        let t = &t;
        async move {
            let (s, v, _) = t
                .req("POST", "/api/v1/projects", Some(&admin), Some(json!({"name": name, "parent_id": parent, "color": if parent.is_none() { Some("#16a34a") } else { None }})))
                .await;
            assert_eq!(s, StatusCode::OK, "{v}");
            v
        }
    };
    let paper = mk("Paper", None).await;
    let pid = paper["id"].as_str().unwrap().to_string();
    let writing = mk("Writing", Some(pid.clone())).await;
    let wid = writing["id"].as_str().unwrap().to_string();
    let draft = mk("Draft", Some(wid.clone())).await;
    let did = draft["id"].as_str().unwrap().to_string();
    assert_eq!(writing["parent_id"], pid);
    assert_eq!(draft["color"], "#16a34a", "subprojects inherit the colour");

    // No cycles: a project can't move under itself or a descendant.
    let (s, _, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/projects/{pid}"),
            Some(&admin),
            Some(json!({"parent_id": did})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/projects/{pid}"),
            Some(&admin),
            Some(json!({"parent_id": pid})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // Moving to the top level and back is fine.
    let (_, v, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/projects/{did}"),
            Some(&admin),
            Some(json!({"parent_id": null})),
        )
        .await;
    assert!(v["parent_id"].is_null());
    let (_, v, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/projects/{did}"),
            Some(&admin),
            Some(json!({"parent_id": wid})),
        )
        .await;
    assert_eq!(v["parent_id"], wid);

    // Another user's project can't be used as a parent.
    let bob = t.user(&admin, "bob").await;
    let (s, _, _) = t
        .req(
            "POST",
            "/api/v1/projects",
            Some(&bob),
            Some(json!({"name": "x", "parent_id": pid})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Archiving cascades down the tree.
    t.req(
        "PATCH",
        &format!("/api/v1/projects/{pid}"),
        Some(&admin),
        Some(json!({"archived": true})),
    )
    .await;
    let (_, v, _) = t.req("GET", "/api/v1/projects", Some(&admin), None).await;
    assert!(
        v.as_array()
            .unwrap()
            .iter()
            .all(|p| p["archived_at"].is_string())
    );
    t.req(
        "PATCH",
        &format!("/api/v1/projects/{pid}"),
        Some(&admin),
        Some(json!({"archived": false})),
    )
    .await;

    // Deleting removes the whole subtree and its tasks.
    let (_, task, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&admin),
            Some(json!({"title": "Write intro", "project_id": did})),
        )
        .await;
    let (s, _, _) = t
        .req(
            "DELETE",
            &format!("/api/v1/projects/{pid}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, v, _) = t.req("GET", "/api/v1/projects", Some(&admin), None).await;
    assert_eq!(v.as_array().unwrap().len(), 0);
    let (s, _, _) = t
        .req(
            "GET",
            &format!("/api/v1/tasks/{}", task["id"].as_str().unwrap()),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
