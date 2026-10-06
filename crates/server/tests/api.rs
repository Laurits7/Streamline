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

#[tokio::test]
async fn profile_preferences() {
    let t = setup().await;
    let admin = t.admin().await;
    let (_, v, _) = t.req("GET", "/api/v1/me", Some(&admin), None).await;
    assert_eq!(
        (v["locale"].as_str(), v["week_start"].as_i64()),
        (Some(""), Some(1))
    );
    let (s, v, _) = t
        .req("PATCH", "/api/v1/me", Some(&admin), Some(json!({"locale": "et-EE", "week_start": 7, "timezone": "Europe/Tallinn", "day_end": "03:30"})))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["locale"], "et-EE");
    assert_eq!(v["week_start"], 7);
    for bad in [
        json!({"locale": "en_GB!"}),
        json!({"week_start": 0}),
        json!({"timezone": "Mars/Base"}),
        json!({"day_end": "25:00"}),
    ] {
        let (s, _, _) = t
            .req("PATCH", "/api/v1/me", Some(&admin), Some(bad.clone()))
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}");
    }
}

#[tokio::test]
async fn openapi_document_covers_the_api() {
    let t = setup().await;
    // Public: no auth needed to read the API description.
    let (s, v, _) = t.req("GET", "/api/v1/openapi.json", None, None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["openapi"].as_str().unwrap().starts_with("3."));
    let paths = v["paths"].as_object().unwrap();
    for p in [
        "/setup",
        "/auth/login",
        "/me",
        "/tokens",
        "/users/{id}",
        "/sync",
        "/events",
        "/today",
        "/projects",
        "/projects/{id}",
        "/tasks",
        "/tasks/{id}",
        "/days/{date}",
        "/days/{date}/entries",
        "/day-entries/{id}",
    ] {
        assert!(paths.contains_key(p), "missing {p}");
    }
    let schemas = v["components"]["schemas"].as_object().unwrap();
    for s in [
        "Task",
        "Project",
        "DayEntry",
        "SyncResponse",
        "CreateTask",
        "PatchProject",
        "Problem",
    ] {
        assert!(schemas.contains_key(s), "missing schema {s}");
    }
    assert!(v["components"]["securitySchemes"]["bearer"].is_object());
}

#[tokio::test]
async fn planning_state_and_free_time() {
    let t = setup().await;
    let admin = t.admin().await;
    let (_, me, _) = t.req("GET", "/api/v1/me", Some(&admin), None).await;
    assert_eq!(me["plan_mode"], "evening");
    assert_eq!(
        (
            me["day_window_start"].as_str(),
            me["day_window_end"].as_str()
        ),
        (Some("08:00"), Some("22:00"))
    );
    for bad in [
        json!({"plan_mode": "noon"}),
        json!({"plan_time_evening": "9pm"}),
        json!({"day_window_start": "22:00"}),
    ] {
        let (s, _, _) = t
            .req("PATCH", "/api/v1/me", Some(&admin), Some(bad.clone()))
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}");
    }
    let (_, v, _) = t.req("GET", "/api/v1/today", Some(&admin), None).await;
    // Free time is measured on a future day (today only counts what's left of it).
    let day = (chrono::NaiveDate::parse_from_str(v["date"].as_str().unwrap(), "%Y-%m-%d").unwrap()
        + chrono::Duration::days(1))
    .format("%Y-%m-%d")
    .to_string();

    // Unplanned day: no plan, the whole 14 h window is free.
    let (_, dv, _) = t
        .req("GET", &format!("/api/v1/days/{day}"), Some(&admin), None)
        .await;
    assert!(dv["plan"].is_null());
    assert_eq!(dv["free_min"], 840);

    // A 90-min scheduled task takes free time; a 30-min unscheduled one counts as planned.
    let (_, a, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&admin),
            Some(json!({"title": "Meeting", "estimate_min": 90, "day": day})),
        )
        .await;
    t.req(
        "POST",
        "/api/v1/tasks",
        Some(&admin),
        Some(json!({"title": "Email", "estimate_min": 30, "day": day})),
    )
    .await;
    let (_, dv, _) = t
        .req("GET", &format!("/api/v1/days/{day}"), Some(&admin), None)
        .await;
    let entry = dv["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["task_id"] == a["id"])
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    t.req(
        "PATCH",
        &format!("/api/v1/day-entries/{entry}"),
        Some(&admin),
        Some(json!({"start_time": "10:00"})),
    )
    .await;
    let (_, dv, _) = t
        .req("GET", &format!("/api/v1/days/{day}"), Some(&admin), None)
        .await;
    assert_eq!(
        (dv["free_min"].as_i64(), dv["planned_min"].as_i64()),
        (Some(750), Some(30))
    );

    // Draft (wizard step 2) -> planned; going back into the wizard keeps it planned.
    let (s, p, _) = t
        .req(
            "PUT",
            &format!("/api/v1/days/{day}/plan"),
            Some(&admin),
            Some(json!({"status": "draft", "step": 2})),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        (p["status"].as_str(), p["step"].as_i64()),
        (Some("draft"), Some(2))
    );
    let (_, p, _) = t
        .req(
            "PUT",
            &format!("/api/v1/days/{day}/plan"),
            Some(&admin),
            Some(json!({"status": "planned", "step": 4})),
        )
        .await;
    assert_eq!(p["status"], "planned");
    assert!(p["planned_at"].is_string());
    let (_, p, _) = t
        .req(
            "PUT",
            &format!("/api/v1/days/{day}/plan"),
            Some(&admin),
            Some(json!({"status": "draft", "step": 1})),
        )
        .await;
    assert_eq!(
        (p["status"].as_str(), p["step"].as_i64()),
        (Some("planned"), Some(1))
    );
    let (s, _, _) = t
        .req(
            "PUT",
            &format!("/api/v1/days/{day}/plan"),
            Some(&admin),
            Some(json!({"status": "done"})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (_, sync, _) = t.req("GET", "/api/v1/sync", Some(&admin), None).await;
    assert_eq!(sync["day_plans"][0]["status"], "planned");

    // Unplanning.
    let (s, _, _) = t
        .req(
            "DELETE",
            &format!("/api/v1/days/{day}/plan"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, dv, _) = t
        .req("GET", &format!("/api/v1/days/{day}"), Some(&admin), None)
        .await;
    assert!(dv["plan"].is_null());
}

#[tokio::test]
async fn planning_reminders_fire_once_and_skip_planned_days() {
    let t = setup().await;
    let admin = t.admin().await;
    // Planning times equal to the day end are always in the past for the current logical day.
    let (_, me, _) = t
        .req("PATCH", "/api/v1/me", Some(&admin), Some(json!({"timezone": "UTC", "day_end": "04:00", "plan_mode": "both", "plan_time_morning": "04:00", "plan_time_evening": "04:00"})))
        .await;
    let user = streamline::auth::load_user(&t.state, me["id"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    let (_, v, _) = t.req("GET", "/api/v1/today", Some(&admin), None).await;
    let today = v["date"].as_str().unwrap().to_string();

    // Today is already planned: only the evening reminder (for tomorrow) goes out.
    t.req(
        "PUT",
        &format!("/api/v1/days/{today}/plan"),
        Some(&admin),
        Some(json!({"status": "planned"})),
    )
    .await;
    let mut events = t.state.bus.subscribe();
    let sent = streamline::reminders::run_for_user(&t.state, &user)
        .await
        .unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].kind, "plan_evening");
    assert!(sent[0].url.starts_with("/plan/") && !sent[0].url.ends_with(&today));
    let ev = events.try_recv().unwrap();
    assert_eq!(ev.kind, "notification");
    assert_eq!(ev.audience, std::slice::from_ref(&user.id));

    // Never twice.
    assert!(
        streamline::reminders::run_for_user(&t.state, &user)
            .await
            .unwrap()
            .is_empty()
    );

    // An unplanned today gets its morning reminder.
    t.req(
        "DELETE",
        &format!("/api/v1/days/{today}/plan"),
        Some(&admin),
        None,
    )
    .await;
    let sent = streamline::reminders::run_for_user(&t.state, &user)
        .await
        .unwrap();
    assert_eq!(
        sent.iter().map(|n| n.kind.as_str()).collect::<Vec<_>>(),
        ["plan_morning"]
    );
}

#[tokio::test]
async fn focus_timer_lifecycle() {
    let t = setup().await;
    let admin = t.admin().await;
    let (_, task, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&admin),
            Some(json!({"title": "Write report", "estimate_min": 60})),
        )
        .await;
    let tid = task["id"].as_str().unwrap().to_string();

    let (_, v, _) = t.req("GET", "/api/v1/focus", Some(&admin), None).await;
    assert_eq!(v["timer"]["phase"], "idle");
    assert!(v["server_now"].as_i64().unwrap() > 0);

    // Start: work runs; the task becomes "in progress".
    let (s, v, _) = t
        .req(
            "POST",
            "/api/v1/focus",
            Some(&admin),
            Some(json!({"action": "start", "task_id": tid})),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        (
            v["timer"]["phase"].as_str(),
            v["timer"]["length_min"].as_i64()
        ),
        (Some("work"), Some(25))
    );
    assert!(v["timer"]["running_since_ms"].is_number());
    let (_, task, _) = t
        .req("GET", &format!("/api/v1/tasks/{tid}"), Some(&admin), None)
        .await;
    assert!(task["started_at"].is_string());

    // Pretend 26 minutes passed: syncing completes the work interval and starts the break.
    sqlx::query("UPDATE focus_timers SET running_since = strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-26 minutes')")
        .execute(&t.state.db.write)
        .await
        .unwrap();
    let (_, v, _) = t
        .req(
            "POST",
            "/api/v1/focus",
            Some(&admin),
            Some(json!({"action": "sync"})),
        )
        .await;
    assert_eq!(v["timer"]["phase"], "short_break");
    assert_eq!(v["timer"]["cycle_done"], 1);
    let (_, task, _) = t
        .req("GET", &format!("/api/v1/tasks/{tid}"), Some(&admin), None)
        .await;
    assert_eq!(
        task["actual_min"], 25,
        "completed work counts as actual time"
    );
    let (_, sync, _) = t.req("GET", "/api/v1/sync", Some(&admin), None).await;
    let sessions = sync["focus_sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(
        (
            sessions[0]["kind"].as_str(),
            sessions[0]["minutes"].as_i64(),
            sessions[0]["completed"].as_bool()
        ),
        (Some("work"), Some(25), Some(true))
    );
    assert_eq!(sync["focus_timer"]["phase"], "short_break");

    // Pause/resume/skip/stop.
    let (_, v, _) = t
        .req(
            "POST",
            "/api/v1/focus",
            Some(&admin),
            Some(json!({"action": "pause"})),
        )
        .await;
    assert!(v["timer"]["running_since_ms"].is_null());
    let (_, v, _) = t
        .req(
            "POST",
            "/api/v1/focus",
            Some(&admin),
            Some(json!({"action": "skip"})),
        )
        .await;
    assert_eq!(
        (
            v["timer"]["phase"].as_str(),
            v["timer"]["running_since_ms"].is_null()
        ),
        (Some("work"), true)
    );
    let (_, v, _) = t
        .req(
            "POST",
            "/api/v1/focus",
            Some(&admin),
            Some(json!({"action": "resume"})),
        )
        .await;
    assert!(v["timer"]["running_since_ms"].is_number());
    let (_, v, _) = t
        .req(
            "POST",
            "/api/v1/focus",
            Some(&admin),
            Some(json!({"action": "stop"})),
        )
        .await;
    assert_eq!(v["timer"]["phase"], "idle");

    // Validation and privacy.
    let (s, _, _) = t
        .req(
            "POST",
            "/api/v1/focus",
            Some(&admin),
            Some(json!({"action": "dance"})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let bob = t.user(&admin, "bob").await;
    let (s, _, _) = t
        .req(
            "POST",
            "/api/v1/focus",
            Some(&bob),
            Some(json!({"action": "start", "task_id": tid})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (_, v, _) = t.req("GET", "/api/v1/focus", Some(&bob), None).await;
    assert_eq!(v["timer"]["phase"], "idle", "each user has their own timer");
}

#[tokio::test]
async fn in_progress_prefs_and_focus_settings() {
    let t = setup().await;
    let admin = t.admin().await;
    let (_, task, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&admin),
            Some(json!({"title": "Paint fence"})),
        )
        .await;
    let tid = task["id"].as_str().unwrap().to_string();
    let (_, v, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/tasks/{tid}"),
            Some(&admin),
            Some(json!({"in_progress": true})),
        )
        .await;
    assert!(v["started_at"].is_string());
    t.req(
        "PATCH",
        &format!("/api/v1/tasks/{tid}"),
        Some(&admin),
        Some(json!({"status": "done"})),
    )
    .await;
    let (_, v, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/tasks/{tid}"),
            Some(&admin),
            Some(json!({"status": "open"})),
        )
        .await;
    assert!(v["started_at"].is_null(), "reopening starts over");

    // Preferences merge by top-level key; null removes.
    t.req(
        "PATCH",
        "/api/v1/me/prefs",
        Some(&admin),
        Some(json!({"view:all": {"view": "board", "group": "status"}, "x": 1})),
    )
    .await;
    let (_, me, _) = t
        .req(
            "PATCH",
            "/api/v1/me/prefs",
            Some(&admin),
            Some(json!({"x": null, "view:inbox": {"view": "matrix"}})),
        )
        .await;
    assert_eq!(
        me["prefs"],
        json!({"view:all": {"view": "board", "group": "status"}, "view:inbox": {"view": "matrix"}})
    );
    let big = "x".repeat(70 * 1024);
    let (s, _, _) = t
        .req(
            "PATCH",
            "/api/v1/me/prefs",
            Some(&admin),
            Some(json!({"big": big})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Pomodoro settings.
    let (_, me, _) = t
        .req(
            "PATCH",
            "/api/v1/me",
            Some(&admin),
            Some(json!({"focus_work_min": 50, "focus_long_every": 3})),
        )
        .await;
    assert_eq!(
        (
            me["focus_work_min"].as_i64(),
            me["focus_long_every"].as_i64()
        ),
        (Some(50), Some(3))
    );
    let (s, _, _) = t
        .req(
            "PATCH",
            "/api/v1/me",
            Some(&admin),
            Some(json!({"focus_work_min": 0})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (_, v, _) = t
        .req(
            "POST",
            "/api/v1/focus",
            Some(&admin),
            Some(json!({"action": "start"})),
        )
        .await;
    assert_eq!(v["timer"]["length_min"], 50);
}

async fn today_of(t: &T, auth: &str) -> chrono::NaiveDate {
    let (_, v, _) = t.req("GET", "/api/v1/today", Some(auth), None).await;
    chrono::NaiveDate::parse_from_str(v["date"].as_str().unwrap(), "%Y-%m-%d").unwrap()
}
fn ymd(d: chrono::NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}
async fn occurrences(t: &T, auth: &str, series_id: &str) -> Vec<Value> {
    let (_, v, _) = t.req("GET", "/api/v1/tasks", Some(auth), None).await;
    let mut out: Vec<Value> = v
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["series_id"] == series_id)
        .cloned()
        .collect();
    out.sort_by_key(|x| x["occurrence_key"].as_str().unwrap().to_string());
    out
}

#[tokio::test]
async fn routines_create_occurrences_once() {
    let t = setup().await;
    let admin = t.admin().await;
    let today = today_of(&t, &admin).await;
    let tomorrow = today + chrono::Duration::days(1);

    // Repeat daily: today and tomorrow exist (tomorrow so it can be planned tonight).
    let (s, daily, _) = t
        .req(
            "POST",
            "/api/v1/series",
            Some(&admin),
            Some(json!({"title": "Water plants", "mode": "repeat", "rrule": "FREQ=DAILY"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{daily}");
    assert_eq!(daily["task_type_id"], "tt_carry_on");
    let occ = occurrences(&t, &admin, daily["id"].as_str().unwrap()).await;
    assert_eq!(
        occ.iter()
            .map(|o| o["occurrence_date"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [ymd(today), ymd(tomorrow)]
    );
    assert_eq!(occ[0]["due_date"], ymd(today));

    // Syncing again doesn't duplicate; a deleted (skipped) occurrence never returns.
    t.req(
        "DELETE",
        &format!("/api/v1/tasks/{}", occ[1]["id"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await;
    t.req("GET", "/api/v1/sync", Some(&admin), None).await;
    sqlx::query("UPDATE series SET materialized_through = NULL")
        .execute(&t.state.db.write)
        .await
        .unwrap();
    t.req("GET", "/api/v1/sync", Some(&admin), None).await;
    assert_eq!(
        occurrences(&t, &admin, daily["id"].as_str().unwrap())
            .await
            .len(),
        1
    );

    // Fixed time: planned on the timeline, "expires" by default.
    let (_, anchored, _) = t
        .req("POST", "/api/v1/series", Some(&admin), Some(json!({"title": "Vitamins", "mode": "anchored", "rrule": "FREQ=DAILY", "start_time": "07:30", "duration_min": 5})))
        .await;
    assert_eq!(anchored["task_type_id"], "tt_expires");
    let (_, day, _) = t
        .req(
            "GET",
            &format!("/api/v1/days/{}", ymd(today)),
            Some(&admin),
            None,
        )
        .await;
    let vit = occurrences(&t, &admin, anchored["id"].as_str().unwrap()).await[0].clone();
    let entry = day["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["task_id"] == vit["id"])
        .expect("planned");
    assert_eq!(
        (entry["start_time"].as_str(), entry["duration_min"].as_i64()),
        (Some("07:30"), Some(5))
    );

    // N times per week: two slots for the current week, due at its end.
    let (_, laundry, _) = t
        .req("POST", "/api/v1/series", Some(&admin), Some(json!({"title": "Laundry", "mode": "flexible", "times_per_window": 2, "window": "week"})))
        .await;
    assert_eq!(laundry["task_type_id"], "tt_window");
    let (ws, we) = streamline_domain::time::week_bounds(today, 1);
    let occ = occurrences(&t, &admin, laundry["id"].as_str().unwrap()).await;
    let this_week: Vec<_> = occ
        .iter()
        .filter(|o| o["occurrence_date"] == ymd(ws))
        .collect();
    assert_eq!(this_week.len(), 2);
    assert_eq!(this_week[0]["window_end"], ymd(we));
    assert_eq!(this_week[0]["due_date"], ymd(we));

    // Window progress counts the done slot, even after next week's slots were created
    // by viewing a day next week.
    t.req(
        "GET",
        &format!("/api/v1/days/{}", ymd(we + chrono::Duration::days(3))),
        Some(&admin),
        None,
    )
    .await;
    t.req(
        "PATCH",
        &format!("/api/v1/tasks/{}", this_week[0]["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({"status": "done"})),
    )
    .await;
    let (_, stats, _) = t
        .req("GET", "/api/v1/series/stats", Some(&admin), None)
        .await;
    let ls = stats
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["series_id"] == laundry["id"])
        .unwrap()
        .clone();
    assert_eq!(
        (ls["window_done"].as_i64(), ls["window_total"].as_i64()),
        (Some(1), Some(2)),
        "{ls}"
    );

    // A later day is filled in on demand when viewed.
    let later = today + chrono::Duration::days(10);
    let (_, day, _) = t
        .req(
            "GET",
            &format!("/api/v1/days/{}", ymd(later)),
            Some(&admin),
            None,
        )
        .await;
    assert!(
        day["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["series_id"] == daily["id"] && x["occurrence_date"] == ymd(later))
    );
    assert!(
        day["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["start_time"] == "07:30")
    );

    // Validation.
    for bad in [
        json!({"title": "x", "mode": "repeat", "rrule": "FREQ=HOURLY"}),
        json!({"title": "x", "mode": "repeat"}),
        json!({"title": "x", "mode": "anchored", "rrule": "FREQ=DAILY"}),
        json!({"title": "x", "mode": "flexible", "times_per_window": 2}),
        json!({"title": "x", "mode": "repeat", "rrule": "FREQ=DAILY", "dtstart": "2026-10-10", "until": "2026-10-01"}),
    ] {
        let (s, _, _) = t
            .req("POST", "/api/v1/series", Some(&admin), Some(bad.clone()))
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}");
    }
}

#[tokio::test]
async fn routine_day_end_rules_and_streaks() {
    let t = setup().await;
    let admin = t.admin().await;
    let today = today_of(&t, &admin).await;
    let yesterday = today - chrono::Duration::days(1);
    let (_, vit, _) = t.req("POST", "/api/v1/series", Some(&admin), Some(json!({"title": "Vitamins", "mode": "repeat", "rrule": "FREQ=DAILY", "task_type_id": "tt_expires"}))).await;
    let (_, laundry, _) = t
        .req("POST", "/api/v1/series", Some(&admin), Some(json!({"title": "Laundry", "mode": "flexible", "times_per_window": 1, "window": "week"})))
        .await;
    let vit_occ = occurrences(&t, &admin, vit["id"].as_str().unwrap()).await;
    let laundry_occ = occurrences(&t, &admin, laundry["id"].as_str().unwrap()).await;

    // Pretend today's vitamins were yesterday's and never planned, and the laundry week ended.
    sqlx::query("UPDATE tasks SET occurrence_date = ?1, due_date = ?1 WHERE id = ?2")
        .bind(ymd(yesterday))
        .bind(vit_occ[0]["id"].as_str().unwrap())
        .execute(&t.state.db.write)
        .await
        .unwrap();
    sqlx::query("UPDATE tasks SET window_end = ? WHERE id = ?")
        .bind(ymd(yesterday))
        .bind(laundry_occ[0]["id"].as_str().unwrap())
        .execute(&t.state.db.write)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET last_rollover_date = NULL")
        .execute(&t.state.db.write)
        .await
        .unwrap();
    t.req("GET", "/api/v1/sync", Some(&admin), None).await;
    let get = |id: &Value| {
        let id = id.as_str().unwrap().to_string();
        let t = &t;
        let admin = admin.clone();
        async move {
            t.req("GET", &format!("/api/v1/tasks/{id}"), Some(&admin), None)
                .await
                .1
        }
    };
    assert_eq!(
        get(&vit_occ[0]["id"]).await["status"],
        "missed",
        "an expiring routine missed without being planned"
    );
    assert_eq!(
        get(&laundry_occ[0]["id"]).await["status"],
        "missed",
        "window ended undone"
    );

    // With overflow = roll, an unfinished window task moves into the current window instead.
    sqlx::query("UPDATE task_types SET window_overflow = 'roll' WHERE id = 'tt_window'")
        .execute(&t.state.db.write)
        .await
        .unwrap();
    let (_, other, _) = t
        .req("POST", "/api/v1/series", Some(&admin), Some(json!({"title": "Vacuum", "mode": "flexible", "times_per_window": 1, "window": "week"})))
        .await;
    let vac = occurrences(&t, &admin, other["id"].as_str().unwrap()).await[0].clone();
    sqlx::query("UPDATE tasks SET window_end = ? WHERE id = ?")
        .bind(ymd(yesterday))
        .bind(vac["id"].as_str().unwrap())
        .execute(&t.state.db.write)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET last_rollover_date = NULL")
        .execute(&t.state.db.write)
        .await
        .unwrap();
    t.req("GET", "/api/v1/sync", Some(&admin), None).await;
    let v = get(&vac["id"]).await;
    assert_eq!(v["status"], "open");
    assert!(v["window_end"].as_str().unwrap() >= ymd(today).as_str());
    assert_eq!(v["carry_count"], 1);

    // Streaks: completing the next vitamins (early) after yesterday's miss = 1.
    let tomorrow = today + chrono::Duration::days(1);
    let today_vit = occurrences(&t, &admin, vit["id"].as_str().unwrap())
        .await
        .into_iter()
        .find(|o| o["occurrence_date"] == ymd(tomorrow))
        .unwrap();
    t.req(
        "PATCH",
        &format!("/api/v1/tasks/{}", today_vit["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({"status": "done"})),
    )
    .await;
    let (_, stats, _) = t
        .req("GET", "/api/v1/series/stats", Some(&admin), None)
        .await;
    let s = stats
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["series_id"] == vit["id"])
        .unwrap();
    assert_eq!(s["streak"], 1);
    assert_eq!(s["next_date"], ymd(today));
}

#[tokio::test]
async fn routine_edits_this_one_vs_all_future() {
    let t = setup().await;
    let admin = t.admin().await;
    let today = today_of(&t, &admin).await;
    let (_, s, _) = t
        .req(
            "POST",
            "/api/v1/series",
            Some(&admin),
            Some(json!({"title": "Stretch", "mode": "repeat", "rrule": "FREQ=DAILY"})),
        )
        .await;
    let sid = s["id"].as_str().unwrap().to_string();
    let occ = occurrences(&t, &admin, &sid).await;

    // "This one": editing the occurrence task only changes it.
    t.req(
        "PATCH",
        &format!("/api/v1/tasks/{}", occ[1]["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({"title": "Stretch (long)"})),
    )
    .await;
    // "All future", content only: in place, open occurrences follow.
    let (_, s2, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/series/{sid}"),
            Some(&admin),
            Some(json!({"title": "Stretching", "estimate_min": 10})),
        )
        .await;
    assert_eq!(s2["id"], sid);
    let occ = occurrences(&t, &admin, &sid).await;
    assert!(
        occ.iter()
            .all(|o| o["title"] == "Stretching" && o["estimate_min"] == 10)
    );

    // Do today's, then change the schedule: the routine splits; history stays with the old one.
    t.req(
        "PATCH",
        &format!("/api/v1/tasks/{}", occ[0]["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({"status": "done"})),
    )
    .await;
    let tomorrow = today + chrono::Duration::days(1);
    let (s, s3, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/series/{sid}"),
            Some(&admin),
            Some(json!({"rrule": "FREQ=WEEKLY", "from": ymd(tomorrow)})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{s3}");
    assert_ne!(s3["id"], sid);
    assert_eq!(s3["dtstart"], ymd(tomorrow));
    let (_, list, _) = t.req("GET", "/api/v1/series", Some(&admin), None).await;
    let old = list
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == sid)
        .unwrap();
    assert_eq!(
        old["until"],
        ymd(today),
        "the old routine ends the day before"
    );
    let old_occ = occurrences(&t, &admin, &sid).await;
    assert_eq!(
        old_occ.len(),
        1,
        "only the done occurrence stays with the old routine"
    );
    assert_eq!(old_occ[0]["status"], "done");
    let new_occ = occurrences(&t, &admin, s3["id"].as_str().unwrap()).await;
    assert_eq!(
        new_occ
            .iter()
            .map(|o| o["occurrence_date"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [ymd(tomorrow)]
    );

    // Changing the schedule "from today" after today's is done doesn't create today's again.
    let (_, daily, _) = t.req("POST", "/api/v1/series", Some(&admin), Some(json!({"title": "Floss", "mode": "anchored", "rrule": "FREQ=DAILY", "start_time": "21:00"}))).await;
    let did = daily["id"].as_str().unwrap().to_string();
    let first = occurrences(&t, &admin, &did).await[0].clone();
    t.req(
        "PATCH",
        &format!("/api/v1/tasks/{}", first["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({"status": "done"})),
    )
    .await;
    let (_, moved, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/series/{did}"),
            Some(&admin),
            Some(json!({"start_time": "22:00"})),
        )
        .await;
    assert_eq!(moved["dtstart"], ymd(today + chrono::Duration::days(1)));
    let all: Vec<Value> = [
        occurrences(&t, &admin, &did).await,
        occurrences(&t, &admin, moved["id"].as_str().unwrap()).await,
    ]
    .concat();
    assert_eq!(
        all.iter()
            .filter(|o| o["occurrence_date"] == ymd(today))
            .count(),
        1,
        "no duplicate for today"
    );

    // Ending a routine removes open occurrences from today on, keeps history.
    let (s, _, _) = t
        .req(
            "DELETE",
            &format!("/api/v1/series/{}", s3["id"].as_str().unwrap()),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(
        occurrences(&t, &admin, s3["id"].as_str().unwrap())
            .await
            .is_empty()
    );
    assert_eq!(occurrences(&t, &admin, &sid).await.len(), 1);
}

#[tokio::test]
async fn tasks_in_several_projects() {
    let t = setup().await;
    let admin = t.admin().await;
    let mk = |name: &'static str| {
        let (t, admin) = (&t, admin.clone());
        async move {
            t.req(
                "POST",
                "/api/v1/projects",
                Some(&admin),
                Some(json!({"name": name})),
            )
            .await
            .1["id"]
                .as_str()
                .unwrap()
                .to_string()
        }
    };
    let (house, garden, cottage) = (mk("House").await, mk("Garden").await, mk("Cottage").await);
    let (_, task, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&admin),
            Some(json!({"title": "Buy garden hose", "project_id": cottage})),
        )
        .await;
    let tid = task["id"].as_str().unwrap().to_string();
    assert_eq!(task["also_project_ids"], json!([]));

    // Duplicates and the main project are ignored; unknown projects rejected.
    let (_, v, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/tasks/{tid}"),
            Some(&admin),
            Some(json!({"also_project_ids": [garden, house, garden, cottage]})),
        )
        .await;
    assert_eq!(v["also_project_ids"], json!([garden, house]));
    let (s, _, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/tasks/{tid}"),
            Some(&admin),
            Some(json!({"also_project_ids": ["nope"]})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Making an "also" project the main one removes it from the extra list.
    let (_, v, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/tasks/{tid}"),
            Some(&admin),
            Some(json!({"project_id": garden})),
        )
        .await;
    assert_eq!(v["also_project_ids"], json!([house]));

    // Deleting a project the task is only also in just unlinks it.
    t.req(
        "DELETE",
        &format!("/api/v1/projects/{house}"),
        Some(&admin),
        None,
    )
    .await;
    let (s, v, _) = t
        .req("GET", &format!("/api/v1/tasks/{tid}"), Some(&admin), None)
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["also_project_ids"], json!([]));
}

#[tokio::test]
async fn daily_activity_log() {
    let t = setup().await;
    let admin = t.admin().await;
    let today = today_of(&t, &admin).await;
    let (_, p, _) = t
        .req(
            "POST",
            "/api/v1/projects",
            Some(&admin),
            Some(json!({"name": "House"})),
        )
        .await;
    let (_, a, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&admin),
            Some(json!({"title": "Fix tap", "project_id": p["id"]})),
        )
        .await;
    let (_, b, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&admin),
            Some(json!({"title": "Call bank"})),
        )
        .await;
    let (_, c, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&admin),
            Some(json!({"title": "Old idea"})),
        )
        .await;
    let id = |v: &Value| v["id"].as_str().unwrap().to_string();
    t.req(
        "PATCH",
        &format!("/api/v1/tasks/{}", id(&a)),
        Some(&admin),
        Some(json!({"status": "done"})),
    )
    .await;
    // Completed then reopened: not logged as completed.
    t.req(
        "PATCH",
        &format!("/api/v1/tasks/{}", id(&b)),
        Some(&admin),
        Some(json!({"status": "done"})),
    )
    .await;
    t.req(
        "PATCH",
        &format!("/api/v1/tasks/{}", id(&b)),
        Some(&admin),
        Some(json!({"status": "open", "in_progress": true})),
    )
    .await;
    t.req(
        "PATCH",
        &format!("/api/v1/tasks/{}", id(&c)),
        Some(&admin),
        Some(json!({"status": "wont_do"})),
    )
    .await;
    // A focus interval (12 min, stopped early).
    t.req(
        "POST",
        "/api/v1/focus",
        Some(&admin),
        Some(json!({"action": "start", "task_id": id(&a)})),
    )
    .await;
    sqlx::query("UPDATE focus_timers SET running_since = strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-12 minutes')").execute(&t.state.db.write).await.unwrap();
    t.req(
        "POST",
        "/api/v1/focus",
        Some(&admin),
        Some(json!({"action": "stop"})),
    )
    .await;
    // Planned tomorrow.
    let tomorrow = ymd(today + chrono::Duration::days(1));
    t.req(
        "PUT",
        &format!("/api/v1/days/{tomorrow}/plan"),
        Some(&admin),
        Some(json!({"status": "planned"})),
    )
    .await;

    let (s, log, _) = t
        .req(
            "GET",
            &format!("/api/v1/days/{}/log", ymd(today)),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let texts: Vec<&str> = log
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["text"].as_str().unwrap())
        .collect();
    assert!(texts.contains(&"Added “Fix tap” (House)"), "{texts:?}");
    assert!(texts.contains(&"Completed “Fix tap” (House)"), "{texts:?}");
    assert!(
        !texts.contains(&"Completed “Call bank”"),
        "reopened completions are left out: {texts:?}"
    );
    assert!(texts.contains(&"Started “Call bank”"), "{texts:?}");
    assert!(texts.contains(&"Decided not to do “Old idea”"), "{texts:?}");
    assert!(
        texts
            .iter()
            .any(|x| x.starts_with("Focused 12 min on “Fix tap”")),
        "{texts:?}"
    );
    assert!(
        texts.iter().any(|x| x.starts_with("Planned the next day")),
        "{texts:?}"
    );
    let ats: Vec<&str> = log
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["at"].as_str().unwrap())
        .collect();
    assert!(ats.windows(2).all(|w| w[0] <= w[1]), "in time order");

    // Another day is empty; another user sees nothing of it.
    let (_, other, _) = t
        .req(
            "GET",
            &format!("/api/v1/days/{tomorrow}/log"),
            Some(&admin),
            None,
        )
        .await;
    assert!(
        other
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["kind"] != "completed")
    );
    let bob = t.user(&admin, "bob").await;
    let (_, theirs, _) = t
        .req(
            "GET",
            &format!("/api/v1/days/{}/log", ymd(today)),
            Some(&bob),
            None,
        )
        .await;
    assert_eq!(theirs, json!([]));
}

#[tokio::test]
async fn changing_how_often_mid_week_tops_up_this_week() {
    let t = setup().await;
    let admin = t.admin().await;
    let today = today_of(&t, &admin).await;
    let (ws, we) = streamline_domain::time::week_bounds(today, 1);
    let (_, old, _) = t
        .req("POST", "/api/v1/series", Some(&admin), Some(json!({"title": "Laundry", "mode": "flexible", "times_per_window": 2, "window": "week", "dtstart": ymd(ws)})))
        .await;
    let oid = old["id"].as_str().unwrap().to_string();
    let this_week = |v: Vec<Value>| {
        v.into_iter()
            .filter(|o| o["occurrence_date"] == ymd(ws))
            .collect::<Vec<_>>()
    };
    let slots = this_week(occurrences(&t, &admin, &oid).await);
    t.req(
        "PATCH",
        &format!("/api/v1/tasks/{}", slots[0]["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({"status": "done"})),
    )
    .await;

    // Twice -> three times a week, from today: one done, so two more this week.
    let (s, new, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/series/{oid}"),
            Some(&admin),
            Some(json!({"times_per_window": 3})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{new}");
    let nid = new["id"].as_str().unwrap().to_string();
    assert_eq!(new["split_from"], oid);
    assert_eq!(
        new["dtstart"],
        ymd(today),
        "takes effect now, not next week"
    );
    let old_now = this_week(occurrences(&t, &admin, &oid).await);
    assert_eq!(
        old_now
            .iter()
            .map(|o| o["status"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["done"],
        "old open slot replaced"
    );
    let new_now = this_week(occurrences(&t, &admin, &nid).await);
    assert_eq!(new_now.len(), 2, "tops up: 3 wanted - 1 done");
    assert!(
        new_now
            .iter()
            .all(|o| o["status"] == "open" && o["window_end"] == ymd(we))
    );

    // Progress and streak count across both versions.
    let (_, stats, _) = t
        .req("GET", "/api/v1/series/stats", Some(&admin), None)
        .await;
    let st = stats
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["series_id"] == nid)
        .unwrap()
        .clone();
    assert_eq!(
        (st["window_done"].as_i64(), st["window_total"].as_i64()),
        (Some(1), Some(3)),
        "{st}"
    );

    // Next week gets the full three (viewing a day there creates them).
    let next_ws = ws + chrono::Duration::days(7);
    t.req(
        "GET",
        &format!("/api/v1/days/{}", ymd(next_ws + chrono::Duration::days(1))),
        Some(&admin),
        None,
    )
    .await;
    let next = occurrences(&t, &admin, &nid)
        .await
        .into_iter()
        .filter(|o| o["occurrence_date"] == ymd(next_ws))
        .count();
    assert_eq!(next, 3);

    // Doing everything this week completes the window and keeps the streak going.
    for o in &new_now {
        t.req(
            "PATCH",
            &format!("/api/v1/tasks/{}", o["id"].as_str().unwrap()),
            Some(&admin),
            Some(json!({"status": "done"})),
        )
        .await;
    }
    let (_, stats, _) = t
        .req("GET", "/api/v1/series/stats", Some(&admin), None)
        .await;
    let st = stats
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["series_id"] == nid)
        .unwrap()
        .clone();
    assert_eq!(
        (st["window_done"].as_i64(), st["streak"].as_i64()),
        (Some(3), Some(1)),
        "{st}"
    );
}

#[tokio::test]
async fn places() {
    let t = setup().await;
    let admin = t.admin().await;
    let (s, cottage, _) = t
        .req(
            "POST",
            "/api/v1/places",
            Some(&admin),
            Some(json!({"name": "Cottage", "lat": 58.38, "lon": 26.72, "radius_m": 300})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{cottage}");
    let (_, town, _) = t
        .req(
            "POST",
            "/api/v1/places",
            Some(&admin),
            Some(json!({"name": "Town"})),
        )
        .await;
    let (cid, tid) = (
        cottage["id"].as_str().unwrap().to_string(),
        town["id"].as_str().unwrap().to_string(),
    );
    for bad in [
        json!({"name": ""}),
        json!({"name": "x", "lat": 10.0}),
        json!({"name": "x", "lat": 95.0, "lon": 0.0}),
        json!({"name": "x", "radius_m": 5}),
    ] {
        let (s, _, _) = t
            .req("POST", "/api/v1/places", Some(&admin), Some(bad.clone()))
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}");
    }

    // A project's default place goes to new tasks in it; a task's own place wins.
    let (_, p, _) = t
        .req(
            "POST",
            "/api/v1/projects",
            Some(&admin),
            Some(json!({"name": "Cottage chores", "default_place_id": cid})),
        )
        .await;
    let (_, a, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&admin),
            Some(json!({"title": "Mow the lawn", "project_id": p["id"]})),
        )
        .await;
    assert_eq!(a["place_id"], cid);
    let (_, b, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&admin),
            Some(json!({"title": "Buy garden hose", "project_id": p["id"], "place_id": tid})),
        )
        .await;
    assert_eq!(b["place_id"], tid);
    let (_, v, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/tasks/{}", a["id"].as_str().unwrap()),
            Some(&admin),
            Some(json!({"place_id": null})),
        )
        .await;
    assert!(v["place_id"].is_null());
    let (s, _, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/tasks/{}", a["id"].as_str().unwrap()),
            Some(&admin),
            Some(json!({"place_id": "nope"})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Routines give their place to each occurrence.
    let (_, r, _) = t.req("POST", "/api/v1/series", Some(&admin), Some(json!({"title": "Water plants", "mode": "repeat", "rrule": "FREQ=DAILY", "place_id": cid}))).await;
    let occ = occurrences(&t, &admin, r["id"].as_str().unwrap()).await;
    assert!(!occ.is_empty() && occ.iter().all(|o| o["place_id"] == cid));

    // Other users can't use my places.
    let bob = t.user(&admin, "bob").await;
    let (s, _, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&bob),
            Some(json!({"title": "x", "place_id": cid})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Deleting a place makes its tasks, projects and routines "anywhere".
    let (s, _, _) = t
        .req(
            "DELETE",
            &format!("/api/v1/places/{cid}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, sync, _) = t.req("GET", "/api/v1/sync", Some(&admin), None).await;
    assert_eq!(sync["places"].as_array().unwrap().len(), 1);
    assert!(
        sync["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["place_id"] != cid)
    );
    assert!(sync["projects"][0]["default_place_id"].is_null());
    assert!(sync["series"][0]["place_id"].is_null());
}

#[tokio::test]
async fn prerequisites_block_and_unblock() {
    let t = setup().await;
    let admin = t.admin().await;
    let mk = |title: &'static str| {
        let (t, admin) = (&t, admin.clone());
        async move {
            t.req(
                "POST",
                "/api/v1/tasks",
                Some(&admin),
                Some(json!({"title": title})),
            )
            .await
            .1["id"]
                .as_str()
                .unwrap()
                .to_string()
        }
    };
    let get = |id: String| {
        let (t, admin) = (&t, admin.clone());
        async move {
            t.req("GET", &format!("/api/v1/tasks/{id}"), Some(&admin), None)
                .await
                .1
        }
    };
    let patch = |id: String, body: Value| {
        let (t, admin) = (&t, admin.clone());
        async move {
            t.req(
                "PATCH",
                &format!("/api/v1/tasks/{id}"),
                Some(&admin),
                Some(body),
            )
            .await
        }
    };
    let (wash, dry, fold) = (mk("Wash").await, mk("Dry").await, mk("Fold").await);

    let (s, v, _) = patch(dry.clone(), json!({"depends_on": [wash], "wait_min": 60})).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["blocked"], true);
    patch(fold.clone(), json!({"depends_on": [dry]})).await;

    // Cycles and self-references are refused.
    let (s, _, _) = patch(wash.clone(), json!({"depends_on": [fold]})).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _, _) = patch(wash.clone(), json!({"depends_on": [wash]})).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Blocked tasks can't be planned.
    let (_, today, _) = t.req("GET", "/api/v1/today", Some(&admin), None).await;
    let day = today["date"].as_str().unwrap();
    let (s, _, _) = t
        .req(
            "POST",
            &format!("/api/v1/days/{day}/entries"),
            Some(&admin),
            Some(json!({"task_id": dry})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Washing unblocks drying, which then waits its hour; folding stays blocked.
    patch(wash.clone(), json!({"status": "done"})).await;
    let d = get(dry.clone()).await;
    assert_eq!(d["blocked"], false);
    assert!(d["ready_at"].as_str().unwrap() > now_iso().as_str());
    assert_eq!(get(fold.clone()).await["blocked"], true);

    // When the wait is over the job releases it and tells the owner.
    sqlx::query("UPDATE tasks SET ready_at = '2000-01-01T00:00:00.000Z' WHERE id = ?")
        .bind(&dry)
        .execute(&t.state.db.write)
        .await
        .unwrap();
    let mut events = t.state.bus.subscribe();
    streamline::deps::release_waiting(&t.state).await.unwrap();
    assert!(get(dry.clone()).await["ready_at"].is_null());
    let mut kinds = vec![];
    while let Ok(e) = events.try_recv() {
        kinds.push((e.kind, e.data["title"].as_str().unwrap_or("").to_string()));
    }
    assert!(
        kinds
            .iter()
            .any(|(k, title)| *k == "notification" && title == "“Dry” is ready"),
        "{kinds:?}"
    );

    // Reopening a prerequisite blocks again; dropping it (won't do / delete) unblocks (D-6).
    patch(wash.clone(), json!({"status": "open"})).await;
    assert_eq!(get(dry.clone()).await["blocked"], true);
    patch(wash.clone(), json!({"status": "wont_do"})).await;
    assert_eq!(get(dry.clone()).await["blocked"], false);
    t.req(
        "DELETE",
        &format!("/api/v1/tasks/{dry}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        get(fold.clone()).await["blocked"],
        false,
        "deleted prerequisite unblocks"
    );
}

#[tokio::test]
async fn blocked_tasks_do_not_miss() {
    let t = setup().await;
    let admin = t.admin().await;
    let today = today_of(&t, &admin).await;
    let (_, gate, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&admin),
            Some(json!({"title": "Buy pills"})),
        )
        .await;
    let (_, r, _) = t.req("POST", "/api/v1/series", Some(&admin), Some(json!({"title": "Take pills", "mode": "repeat", "rrule": "FREQ=DAILY", "task_type_id": "tt_expires"}))).await;
    let occ = occurrences(&t, &admin, r["id"].as_str().unwrap()).await[0].clone();
    let oid = occ["id"].as_str().unwrap().to_string();
    t.req(
        "PATCH",
        &format!("/api/v1/tasks/{oid}"),
        Some(&admin),
        Some(json!({"depends_on": [gate["id"]]})),
    )
    .await;
    sqlx::query("UPDATE tasks SET occurrence_date = ? WHERE id = ?")
        .bind(ymd(today - chrono::Duration::days(1)))
        .bind(&oid)
        .execute(&t.state.db.write)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET last_rollover_date = NULL")
        .execute(&t.state.db.write)
        .await
        .unwrap();
    t.req("GET", "/api/v1/sync", Some(&admin), None).await;
    let (_, v, _) = t
        .req("GET", &format!("/api/v1/tasks/{oid}"), Some(&admin), None)
        .await;
    assert_eq!(
        v["status"], "open",
        "blocked while its day passed: not missed"
    );
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[tokio::test]
async fn workflow_templates_start_chains() {
    let t = setup().await;
    let admin = t.admin().await;
    let laundry = json!({
        "name": "Laundry",
        "steps": [
            {"id": "wash", "title": "Wash", "wait_min": 60, "estimate_min": 5},
            {"id": "dry", "title": "Dry", "wait_min": 45},
            {"id": "iron", "title": "Iron"},
            {"id": "fold", "title": "Fold"}
        ],
        "variants": [
            {"id": "whites", "name": "Whites", "skip": []},
            {"id": "delicates", "name": "Delicates", "skip": ["dry", "iron"]},
            {"id": "towels", "name": "Towels", "skip": ["iron"]}
        ]
    });
    let (s, w, _) = t
        .req(
            "POST",
            "/api/v1/workflows",
            Some(&admin),
            Some(laundry.clone()),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{w}");
    let wid = w["id"].as_str().unwrap().to_string();

    // Validation.
    for bad in [
        json!({"name": "x", "steps": []}),
        json!({"name": "x", "steps": [{"id": "a", "title": "A"}, {"id": "a", "title": "B"}]}),
        json!({"name": "x", "steps": [{"id": "a", "title": "A"}], "variants": [{"id": "v", "name": "V", "skip": ["zzz"]}]}),
        json!({"name": "x", "steps": [{"id": "a", "title": "A"}], "variants": [{"id": "v", "name": "V", "skip": ["a"]}]}),
    ] {
        let (s, _, _) = t
            .req("POST", "/api/v1/workflows", Some(&admin), Some(bad.clone()))
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}");
    }

    // Two loads: delicates (wash -> fold) and towels (wash -> dry -> fold), first steps planned today.
    let today = ymd(today_of(&t, &admin).await);
    let (s, tasks, _) = t
        .req(
            "POST",
            &format!("/api/v1/workflows/{wid}/start"),
            Some(&admin),
            Some(json!({"variant_ids": ["delicates", "towels"], "day": today})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{tasks}");
    let tasks = tasks.as_array().unwrap();
    let titles: Vec<&str> = tasks.iter().map(|x| x["title"].as_str().unwrap()).collect();
    assert_eq!(
        titles,
        [
            "Wash (Delicates)",
            "Fold (Delicates)",
            "Wash (Towels)",
            "Dry (Towels)",
            "Fold (Towels)"
        ]
    );
    assert_eq!(
        tasks
            .iter()
            .map(|x| x["blocked"].as_bool().unwrap())
            .collect::<Vec<_>>(),
        [false, true, false, true, true]
    );
    assert_eq!(tasks[1]["depends_on"], json!([tasks[0]["id"]]));
    assert_eq!(
        tasks[1]["wait_min"], 60,
        "fold waits for the wash to finish"
    );
    assert_eq!(tasks[4]["wait_min"], 45, "after the dryer");
    assert_eq!(
        (
            tasks[3]["workflow_step"].as_i64(),
            tasks[3]["workflow_steps"].as_i64()
        ),
        (Some(2), Some(3))
    );
    assert_ne!(
        tasks[0]["workflow_instance_id"], tasks[2]["workflow_instance_id"],
        "one run per load"
    );
    let (_, day, _) = t
        .req("GET", &format!("/api/v1/days/{today}"), Some(&admin), None)
        .await;
    assert_eq!(
        day["entries"].as_array().unwrap().len(),
        2,
        "the first step of each load is planned"
    );

    // Washing the delicates makes folding them ready after the hour.
    let wash = tasks[0]["id"].as_str().unwrap();
    t.req(
        "PATCH",
        &format!("/api/v1/tasks/{wash}"),
        Some(&admin),
        Some(json!({"status": "done"})),
    )
    .await;
    let (_, fold, _) = t
        .req(
            "GET",
            &format!("/api/v1/tasks/{}", tasks[1]["id"].as_str().unwrap()),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(fold["blocked"], false);
    assert!(fold["ready_at"].is_string());

    // Without variants chosen, the whole template runs once.
    let (_, all, _) = t
        .req(
            "POST",
            &format!("/api/v1/workflows/{wid}/start"),
            Some(&admin),
            Some(json!({})),
        )
        .await;
    assert_eq!(all.as_array().unwrap().len(), 4);
    let (s, _, _) = t
        .req(
            "POST",
            &format!("/api/v1/workflows/{wid}/start"),
            Some(&admin),
            Some(json!({"variant_ids": ["nope"]})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn routine_week_score_and_rescheduling() {
    let t = setup().await;
    let admin = t.admin().await;
    let today = today_of(&t, &admin).await;
    let (ws, _) = streamline_domain::time::week_bounds(today, 1);
    // Every day this week: 7 planned by the schedule.
    let (_, r, _) = t
        .req("POST", "/api/v1/series", Some(&admin), Some(json!({"title": "Work out", "mode": "repeat", "rrule": "FREQ=DAILY", "dtstart": ymd(ws), "task_type_id": "tt_expires"})))
        .await;
    let rid = r["id"].as_str().unwrap().to_string();
    let score = || {
        let (t, admin, rid) = (&t, admin.clone(), rid.clone());
        async move {
            let (_, stats, _) = t
                .req("GET", "/api/v1/series/stats", Some(&admin), None)
                .await;
            let s = stats
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["series_id"] == rid)
                .unwrap()
                .clone();
            (
                s["week_done"].as_i64().unwrap(),
                s["week_total"].as_i64().unwrap(),
            )
        }
    };
    assert_eq!(score().await, (0, 7));
    let occ = occurrences(&t, &admin, &rid).await;
    let today_occ = occ
        .iter()
        .find(|o| o["occurrence_date"] == ymd(today))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    t.req(
        "PATCH",
        &format!("/api/v1/tasks/{today_occ}"),
        Some(&admin),
        Some(json!({"status": "done"})),
    )
    .await;
    assert_eq!(score().await, (1, 7));
    // Deleting (or skipping) an occurrence doesn't shrink the week: still out of 7.
    if let Some(next) = occ.iter().find(|o| o["occurrence_date"] != ymd(today)) {
        t.req(
            "DELETE",
            &format!("/api/v1/tasks/{}", next["id"].as_str().unwrap()),
            Some(&admin),
            None,
        )
        .await;
    }
    assert_eq!(score().await.1, 7);

    // An "expires" occurrence moved to a later day isn't missed on its original day.
    let (_, other, _) = t
        .req("POST", "/api/v1/series", Some(&admin), Some(json!({"title": "Swim", "mode": "repeat", "rrule": "FREQ=DAILY", "task_type_id": "tt_expires"})))
        .await;
    let swim = occurrences(&t, &admin, other["id"].as_str().unwrap()).await[0].clone();
    let sid = swim["id"].as_str().unwrap().to_string();
    let tomorrow = ymd(today + chrono::Duration::days(1));
    t.req(
        "PATCH",
        &format!("/api/v1/tasks/{sid}"),
        Some(&admin),
        Some(json!({"due_date": tomorrow})),
    )
    .await;
    sqlx::query("UPDATE tasks SET occurrence_date = ? WHERE id = ?")
        .bind(ymd(today - chrono::Duration::days(1)))
        .bind(&sid)
        .execute(&t.state.db.write)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET last_rollover_date = NULL")
        .execute(&t.state.db.write)
        .await
        .unwrap();
    t.req("GET", "/api/v1/sync", Some(&admin), None).await;
    let (_, v, _) = t
        .req("GET", &format!("/api/v1/tasks/{sid}"), Some(&admin), None)
        .await;
    assert_eq!(
        v["status"], "open",
        "moved to tomorrow: not missed yesterday"
    );
}

#[tokio::test]
async fn routines_start_workflow_runs() {
    let t = setup().await;
    let admin = t.admin().await;
    let today = today_of(&t, &admin).await;
    let yesterday = today - chrono::Duration::days(1);
    let (_, w, _) = t
        .req("POST", "/api/v1/workflows", Some(&admin), Some(json!({"name": "Laundry", "steps": [{"id": "wash", "title": "Wash", "wait_min": 60}, {"id": "fold", "title": "Fold"}]})))
        .await;
    // A daily laundry routine that started yesterday (as if the server had been running).
    let (s, r, _) = t
        .req("POST", "/api/v1/series", Some(&admin), Some(json!({"title": "Laundry", "mode": "repeat", "rrule": "FREQ=DAILY", "dtstart": ymd(yesterday), "workflow_template_id": w["id"]})))
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    let rid = r["id"].as_str().unwrap().to_string();
    sqlx::query("UPDATE series SET materialized_through = ? WHERE id = ?")
        .bind(ymd(yesterday - chrono::Duration::days(1)))
        .bind(&rid)
        .execute(&t.state.db.write)
        .await
        .unwrap();
    // Remove what creation made for today, to replay yesterday -> today in order.
    sqlx::query("DELETE FROM tasks WHERE series_id = ?")
        .bind(&rid)
        .execute(&t.state.db.write)
        .await
        .unwrap();
    t.req("GET", "/api/v1/sync", Some(&admin), None).await;

    let occ = occurrences(&t, &admin, &rid).await;
    let on = |d: chrono::NaiveDate| {
        occ.iter()
            .filter(|o| o["occurrence_date"] == ymd(d))
            .cloned()
            .collect::<Vec<_>>()
    };
    let y = on(yesterday);
    assert_eq!(
        y.iter()
            .map(|o| o["title"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["Wash", "Fold"],
        "yesterday's run"
    );
    assert_eq!(y[1]["depends_on"], json!([y[0]["id"]]));
    let td = on(today);
    assert_eq!(td.len(), 1);
    assert_eq!(
        td[0]["status"], "skipped",
        "previous run unfinished: today's is skipped (D-7)"
    );
    assert!(td[0]["notes"].as_str().unwrap().contains("wasn't finished"));
    // Runs start on their day, not the day before.
    assert!(on(today + chrono::Duration::days(1)).is_empty());

    // Finishing yesterday's run: that day counts as done.
    for o in &y {
        t.req(
            "PATCH",
            &format!("/api/v1/tasks/{}", o["id"].as_str().unwrap()),
            Some(&admin),
            Some(json!({"status": "done"})),
        )
        .await;
    }
    let (_, stats, _) = t
        .req("GET", "/api/v1/series/stats", Some(&admin), None)
        .await;
    let st = stats
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["series_id"] == rid)
        .unwrap()
        .clone();
    assert_eq!(st["streak"], 1, "{st}");
}

/// Phase 4 gate: who sees what. Anna owns the Family group, Ben is a member, Cid isn't,
/// and the admin isn't a member either (admins manage accounts, not other people's tasks).
#[tokio::test]
async fn group_visibility_matrix() {
    let t = setup().await;
    let admin = t.admin().await;
    let anna = t.user(&admin, "anna").await;
    let ben = t.user(&admin, "ben").await;
    let cid = t.user(&admin, "cid").await;
    let id_of = |v: &Value| v["id"].as_str().unwrap().to_string();
    let (_, me_ben, _) = t.req("GET", "/api/v1/me", Some(&ben), None).await;
    let ben_id = id_of(&me_ben);

    // Group + members.
    let (_, fam, _) = t
        .req(
            "POST",
            "/api/v1/groups",
            Some(&anna),
            Some(json!({"name": "Family"})),
        )
        .await;
    let gid = id_of(&fam);
    let (s, _, _) = t
        .req(
            "POST",
            &format!("/api/v1/groups/{gid}/members"),
            Some(&ben),
            Some(json!({"user_id": ben_id})),
        )
        .await;
    assert_eq!(
        s,
        StatusCode::NOT_FOUND,
        "non-members can't manage the group"
    );
    let (s, g, _) = t
        .req(
            "POST",
            &format!("/api/v1/groups/{gid}/members"),
            Some(&anna),
            Some(json!({"user_id": ben_id})),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(g["members"].as_array().unwrap().len(), 2);
    let (_, me_cid, _) = t.req("GET", "/api/v1/me", Some(&cid), None).await;
    let (s, _, _) = t
        .req(
            "POST",
            &format!("/api/v1/groups/{gid}/members"),
            Some(&ben),
            Some(json!({"user_id": id_of(&me_cid)})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "members can't add people");

    // Shared and personal things.
    let (_, house, _) = t
        .req(
            "POST",
            "/api/v1/projects",
            Some(&anna),
            Some(json!({"name": "House", "owner_group_id": gid})),
        )
        .await;
    assert_eq!(house["owner_group_id"], gid);
    let (_, sub, _) = t
        .req(
            "POST",
            "/api/v1/projects",
            Some(&anna),
            Some(json!({"name": "Kitchen", "parent_id": house["id"]})),
        )
        .await;
    assert_eq!(
        sub["owner_group_id"], gid,
        "subprojects follow their parent"
    );
    let (_, chore, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&anna),
            Some(json!({"title": "Take out garbage", "project_id": sub["id"]})),
        )
        .await;
    assert_eq!(chore["owner_group_id"], gid);
    let (_, diary, _) = t
        .req(
            "POST",
            "/api/v1/projects",
            Some(&anna),
            Some(json!({"name": "Diary"})),
        )
        .await;
    let (_, private, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&anna),
            Some(json!({"title": "Write diary", "project_id": diary["id"]})),
        )
        .await;
    let (_, loose, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&anna),
            Some(json!({"title": "Buy milk", "owner_group_id": gid})),
        )
        .await;
    assert_eq!(loose["owner_group_id"], gid);
    let (s, _, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&cid),
            Some(json!({"title": "x", "owner_group_id": gid})),
        )
        .await;
    assert_eq!(
        s,
        StatusCode::BAD_REQUEST,
        "only members can share with a group"
    );
    let (_, bins, _) = t.req("POST", "/api/v1/series", Some(&anna), Some(json!({"title": "Bins out", "mode": "repeat", "rrule": "FREQ=DAILY", "project_id": house["id"]}))).await;
    assert_eq!(bins["owner_group_id"], gid);

    let titles = |v: &Value, key: &str| -> Vec<String> {
        let mut out: Vec<String> = v[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| {
                x[if key == "projects" { "name" } else { "title" }]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        out.sort();
        out
    };
    let (_, a, _) = t.req("GET", "/api/v1/sync", Some(&anna), None).await;
    let (_, b, _) = t.req("GET", "/api/v1/sync", Some(&ben), None).await;
    let (_, c, _) = t.req("GET", "/api/v1/sync", Some(&cid), None).await;
    let (_, adm, _) = t.req("GET", "/api/v1/sync", Some(&admin), None).await;
    assert_eq!(titles(&a, "projects"), ["Diary", "House", "Kitchen"]);
    assert_eq!(
        titles(&b, "projects"),
        ["House", "Kitchen"],
        "member sees group projects only"
    );
    assert!(titles(&c, "projects").is_empty() && titles(&adm, "projects").is_empty());
    assert!(
        titles(&b, "tasks").contains(&"Take out garbage".to_string())
            && titles(&b, "tasks").contains(&"Buy milk".to_string())
    );
    assert!(!titles(&b, "tasks").contains(&"Write diary".to_string()));
    assert!(titles(&c, "tasks").is_empty() && titles(&adm, "tasks").is_empty());
    assert_eq!(b["series"].as_array().unwrap().len(), 1);
    assert_eq!(b["groups"][0]["name"], "Family");
    assert!(c["groups"].as_array().unwrap().is_empty());
    assert_eq!(
        adm["groups"].as_array().unwrap().len(),
        1,
        "admins see groups to manage them"
    );

    // The recurring group chore has one occurrence per day for everyone.
    let bins_today = |v: &Value| {
        v["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|x| x["series_id"] == bins["id"])
            .count()
    };
    assert_eq!(bins_today(&a), bins_today(&b));
    assert!(bins_today(&a) >= 1);

    // Direct access by non-members fails.
    for (who, path) in [
        (&cid, format!("/api/v1/tasks/{}", id_of(&chore))),
        (&admin, format!("/api/v1/tasks/{}", id_of(&chore))),
        (&ben, format!("/api/v1/tasks/{}", id_of(&private))),
    ] {
        let (s, _, _) = t.req("GET", &path, Some(who), None).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{path}");
    }
    let (s, _, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/tasks/{}", id_of(&chore)),
            Some(&cid),
            Some(json!({"status": "done"})),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Both members can put the group task on their own day plan; plans stay private.
    let day = a["today"].as_str().unwrap().to_string();
    for who in [&anna, &ben] {
        let (s, _, _) = t
            .req(
                "POST",
                &format!("/api/v1/days/{day}/entries"),
                Some(who),
                Some(json!({"task_id": id_of(&chore)})),
            )
            .await;
        assert_eq!(s, StatusCode::OK);
    }
    let (_, b2, _) = t.req("GET", "/api/v1/sync", Some(&ben), None).await;
    assert_eq!(
        b2["day_entries"].as_array().unwrap().len(),
        1,
        "only Ben's own entry"
    );

    // Any member completes it, and who did is recorded.
    let (s, done, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/tasks/{}", id_of(&chore)),
            Some(&ben),
            Some(json!({"status": "done"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(done["completed_by"], ben_id);
    let (_, seen, _) = t
        .req(
            "GET",
            &format!("/api/v1/tasks/{}", id_of(&chore)),
            Some(&anna),
            None,
        )
        .await;
    assert_eq!(
        (seen["status"].as_str(), seen["completed_by"].as_str()),
        (Some("done"), Some(ben_id.as_str()))
    );

    // Personal data stays personal: Ben's log has his completion, Anna's doesn't show it as hers.
    let (_, blog, _) = t
        .req("GET", &format!("/api/v1/days/{day}/log"), Some(&ben), None)
        .await;
    assert!(
        blog.as_array()
            .unwrap()
            .iter()
            .any(|x| x["kind"] == "completed")
    );
    assert!(
        !blog
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["title"] == "Write diary")
    );
    let (_, focus_b, _) = t.req("GET", "/api/v1/focus", Some(&ben), None).await;
    assert_eq!(focus_b["timer"]["phase"], "idle");

    // Unsharing takes it away from Ben; a group with content can't be deleted.
    let (s, _, _) = t
        .req(
            "DELETE",
            &format!("/api/v1/groups/{gid}"),
            Some(&anna),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/projects/{}", id_of(&sub)),
            Some(&anna),
            Some(json!({"owner_group_id": null})),
        )
        .await;
    assert_eq!(
        s,
        StatusCode::BAD_REQUEST,
        "subprojects are shared with their top-level project"
    );
    t.req(
        "PATCH",
        &format!("/api/v1/projects/{}", id_of(&house)),
        Some(&anna),
        Some(json!({"owner_group_id": null})),
    )
    .await;
    let (_, b3, _) = t.req("GET", "/api/v1/sync", Some(&ben), None).await;
    assert!(titles(&b3, "projects").is_empty());
    assert_eq!(titles(&b3, "tasks"), ["Buy milk"]);
    let (_, a3, _) = t.req("GET", "/api/v1/sync", Some(&anna), None).await;
    assert!(
        a3["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|x| x["project_id"] == sub["id"])
            .all(|x| x["owner_user_id"].is_string())
    );

    // Leaving the group: Ben no longer sees the shared inbox task.
    let (s, _, _) = t
        .req(
            "DELETE",
            &format!("/api/v1/groups/{gid}/members/{ben_id}"),
            Some(&ben),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, b4, _) = t.req("GET", "/api/v1/sync", Some(&ben), None).await;
    assert!(titles(&b4, "tasks").is_empty());
    assert!(b4["groups"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn group_changes_stream_to_members_only() {
    use http_body_util::BodyExt;
    let t = setup().await;
    let admin = t.admin().await;
    let anna = t.user(&admin, "anna").await;
    let ben = t.user(&admin, "ben").await;
    let cid = t.user(&admin, "cid").await;
    let (_, me_ben, _) = t.req("GET", "/api/v1/me", Some(&ben), None).await;
    let (_, fam, _) = t
        .req(
            "POST",
            "/api/v1/groups",
            Some(&anna),
            Some(json!({"name": "Family"})),
        )
        .await;
    let gid = fam["id"].as_str().unwrap().to_string();
    t.req(
        "POST",
        &format!("/api/v1/groups/{gid}/members"),
        Some(&anna),
        Some(json!({"user_id": me_ben["id"]})),
    )
    .await;

    // Open event streams for Ben (member) and Cid (not).
    let open = |who: String| {
        let app = t.app.clone();
        async move {
            let mut req = Request::builder()
                .uri("/api/v1/events")
                .header(header::HOST, "localhost:3000")
                .header(header::COOKIE, format!("sl_session={who}"))
                .body(Body::empty())
                .unwrap();
            req.extensions_mut()
                .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 1))));
            app.oneshot(req).await.unwrap().into_body()
        }
    };
    let mut ben_stream = open(ben.clone()).await;
    let mut cid_stream = open(cid.clone()).await;
    // Anna adds a group chore.
    let (_, chore, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&anna),
            Some(json!({"title": "Take out garbage", "owner_group_id": gid})),
        )
        .await;
    let id = chore["id"].as_str().unwrap().to_string();

    async fn read_until(body: &mut Body, needle: &str) -> bool {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(800);
        let mut seen = String::new();
        while let Ok(Some(Ok(frame))) = tokio::time::timeout_at(deadline, body.frame()).await {
            if let Some(data) = frame.data_ref() {
                seen.push_str(&String::from_utf8_lossy(data));
                if seen.contains(needle) {
                    return true;
                }
            }
        }
        false
    }
    assert!(
        read_until(&mut ben_stream, &id).await,
        "the member hears about it live"
    );
    assert!(
        !read_until(&mut cid_stream, &id).await,
        "a non-member doesn't"
    );
}

#[tokio::test]
async fn shared_fixed_time_routines_land_on_every_members_timeline() {
    let t = setup().await;
    let admin = t.admin().await;
    let anna = t.user(&admin, "anna").await;
    let ben = t.user(&admin, "ben").await;
    let (_, me_ben, _) = t.req("GET", "/api/v1/me", Some(&ben), None).await;
    let (_, fam, _) = t
        .req(
            "POST",
            "/api/v1/groups",
            Some(&anna),
            Some(json!({"name": "Family"})),
        )
        .await;
    let gid = fam["id"].as_str().unwrap().to_string();
    t.req(
        "POST",
        &format!("/api/v1/groups/{gid}/members"),
        Some(&anna),
        Some(json!({"user_id": me_ben["id"]})),
    )
    .await;
    let (_, walk, _) = t
        .req("POST", "/api/v1/series", Some(&anna), Some(json!({"title": "Walk the dog", "mode": "anchored", "rrule": "FREQ=DAILY", "start_time": "07:30", "owner_group_id": gid})))
        .await;
    let (_, today, _) = t.req("GET", "/api/v1/today", Some(&anna), None).await;
    let day = today["date"].as_str().unwrap().to_string();
    let entry_of = |v: &Value| {
        v["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["start_time"] == "07:30")
            .cloned()
    };
    let (_, a_day, _) = t
        .req("GET", &format!("/api/v1/days/{day}"), Some(&anna), None)
        .await;
    let (_, b_day, _) = t
        .req("GET", &format!("/api/v1/days/{day}"), Some(&ben), None)
        .await;
    let (a, b) = (
        entry_of(&a_day).expect("on Anna's timeline"),
        entry_of(&b_day).expect("on Ben's timeline"),
    );
    assert_eq!(a["task_id"], b["task_id"], "one shared occurrence");
    assert_ne!(a["id"], b["id"], "each member has their own entry");
    assert!(
        b_day["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["series_id"] == walk["id"])
    );
    // Ben walks the dog: done for both.
    t.req(
        "PATCH",
        &format!("/api/v1/tasks/{}", a["task_id"].as_str().unwrap()),
        Some(&ben),
        Some(json!({"status": "done"})),
    )
    .await;
    let (_, task, _) = t
        .req(
            "GET",
            &format!("/api/v1/tasks/{}", a["task_id"].as_str().unwrap()),
            Some(&anna),
            None,
        )
        .await;
    assert_eq!(task["status"], "done");
    assert_eq!(task["completed_by"], me_ben["id"]);
}

// ----- Calendar (Phase 5): a mock CalDAV server -----

#[derive(Default)]
struct Dav {
    ctag: u32,
    /// href → (etag, ics)
    objects: std::collections::BTreeMap<String, (String, String)>,
    requests: Vec<String>,
}

type DavState = std::sync::Arc<std::sync::Mutex<Dav>>;

async fn dav_handler(
    axum::extract::State(dav): axum::extract::State<DavState>,
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
    body: String,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    // anna:secret
    if headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        != Some("Basic YW5uYTpzZWNyZXQ=")
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut d = dav.lock().unwrap();
    let path = uri.path().to_string();
    let kind = [
        "current-user-principal",
        "calendar-home-set",
        "calendar-multiget",
        "calendar-query",
        "resourcetype",
    ]
    .into_iter()
    .find(|k| body.contains(k))
    .unwrap_or("?");
    d.requests.push(format!("{method} {path} {kind}"));
    let ms = |inner: String| {
        (
            StatusCode::MULTI_STATUS,
            [(header::CONTENT_TYPE, "application/xml")],
            format!(r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/">{inner}</d:multistatus>"#),
        )
            .into_response()
    };
    let ok = |href: &str, props: &str| {
        format!(
            "<d:response><d:href>{href}</d:href><d:propstat><d:prop>{props}</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>"
        )
    };
    match (method.as_str(), path.as_str(), kind) {
        ("PROPFIND", "/dav/", "resourcetype") => ms(ok(
            "/dav/",
            "<d:resourcetype><d:collection/></d:resourcetype>",
        )),
        ("PROPFIND", "/dav/", "current-user-principal") => ms(ok(
            "/dav/",
            "<d:current-user-principal><d:href>/dav/anna/</d:href></d:current-user-principal>",
        )),
        ("PROPFIND", "/dav/anna/", "calendar-home-set") => ms(ok(
            "/dav/anna/",
            "<c:calendar-home-set><d:href>/dav/anna/</d:href></c:calendar-home-set>",
        )),
        ("PROPFIND", "/dav/anna/", "resourcetype") => ms(ok(
            "/dav/anna/",
            "<d:resourcetype><d:collection/></d:resourcetype>",
        ) + &ok(
            "/dav/anna/work/",
            &format!(
                "<d:resourcetype><d:collection/><c:calendar/></d:resourcetype><d:displayname>Work</d:displayname><cs:getctag>{}</cs:getctag>",
                d.ctag
            ),
        )),
        ("REPORT", "/dav/anna/work/", "calendar-query") => ms(d
            .objects
            .iter()
            .map(|(h, (e, _))| ok(h, &format!("<d:getetag>{e}</d:getetag>")))
            .collect()),
        ("REPORT", "/dav/anna/work/", "calendar-multiget") => ms(d
            .objects
            .iter()
            .filter(|(h, _)| body.contains(&format!("<d:href>{h}</d:href>")))
            .map(|(h, (e, ics))| {
                ok(
                    h,
                    &format!(
                        "<d:getetag>{e}</d:getetag><c:calendar-data>{}</c:calendar-data>",
                        ics.replace('&', "&amp;").replace('<', "&lt;")
                    ),
                )
            })
            .collect()),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn mock_dav() -> (String, DavState) {
    let dav = DavState::default();
    let app = Router::new().fallback(dav_handler).with_state(dav.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}/dav/"), dav)
}

fn vevent(uid: &str, start: chrono::DateTime<chrono::Utc>, extra: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//test//EN\r\nBEGIN:VEVENT\r\nUID:{uid}\r\nDTSTAMP:20260101T000000Z\r\nDTSTART:{}\r\nDURATION:PT1H\r\n{extra}END:VEVENT\r\nEND:VCALENDAR\r\n",
        start.format("%Y%m%dT%H%M%SZ")
    )
}

#[tokio::test]
async fn calendar_account_sync_and_privacy() {
    let t = setup().await;
    let admin = t.admin().await;
    let anna = t.user(&admin, "anna").await;
    let (url, dav) = mock_dav().await;
    let tomorrow = (chrono::Utc::now() + chrono::Duration::days(1))
        .date_naive()
        .and_hms_opt(9, 0, 0)
        .unwrap()
        .and_utc();
    {
        let mut d = dav.lock().unwrap();
        d.ctag = 1;
        d.objects.insert(
            "/dav/anna/work/a.ics".into(),
            (
                "\"1\"".into(),
                vevent("a", tomorrow, "SUMMARY:Dentist & checkup\r\n"),
            ),
        );
        d.objects.insert(
            "/dav/anna/work/b.ics".into(),
            (
                "\"1\"".into(),
                vevent(
                    "b",
                    tomorrow,
                    "SUMMARY:Standup\r\nRRULE:FREQ=DAILY;COUNT=3\r\n",
                ),
            ),
        );
    }

    // Test connection first: nothing saved.
    let (s, v, _) = t
        .req(
            "POST",
            "/api/v1/calendar/test",
            Some(&anna),
            Some(json!({"url": url, "username": "anna", "password": "wrong"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["ok"], false);
    assert!(v["error"].as_str().unwrap().contains("rejected"), "{v}");
    let (_, v, _) = t
        .req(
            "POST",
            "/api/v1/calendar/test",
            Some(&anna),
            Some(json!({"url": url, "username": "anna", "password": "secret"})),
        )
        .await;
    assert_eq!(v["calendars"], json!(["Work"]));
    let (_, v, _) = t
        .req("GET", "/api/v1/calendar/account", Some(&anna), None)
        .await;
    assert_eq!(v, Value::Null);

    // Credentials in the URL are split off; the password is never returned.
    let with_creds = url.replace("http://", "http://anna:secret@");
    let (s, acc, _) = t
        .req(
            "PUT",
            "/api/v1/calendar/account",
            Some(&anna),
            Some(json!({"url": with_creds})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{acc}");
    assert_eq!(acc["status"], "ok", "{acc}");
    assert_eq!(acc["url"], url);
    assert_eq!(acc["username"], "anna");
    assert_eq!(acc["has_password"], true);
    let (_, full, _) = t.req("GET", "/api/v1/sync", Some(&anna), None).await;
    assert!(!full.to_string().contains("secret"));
    assert_eq!(full["calendar_account"]["status"], "ok");
    assert_eq!(full["calendars"].as_array().unwrap().len(), 1);
    let events = full["events"].as_array().unwrap();
    assert_eq!(events.len(), 4, "one dentist + three standups: {events:?}");
    let dentist = events.iter().find(|e| e["uid"] == "a").unwrap();
    assert_eq!(dentist["title"], "Dentist & checkup");
    assert_eq!(
        dentist["start_at"],
        tomorrow.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    );
    let rev = full["rev"].as_i64().unwrap();

    // The day view lists the day's events, and busy events reduce free time (overlaps once).
    let day = |d: chrono::DateTime<chrono::Utc>| format!("/api/v1/days/{}", d.format("%Y-%m-%d"));
    let (_, with, _) = t.req("GET", &day(tomorrow), Some(&anna), None).await;
    let (_, without, _) = t
        .req(
            "GET",
            &day(tomorrow + chrono::Duration::days(5)),
            Some(&anna),
            None,
        )
        .await;
    assert_eq!(with["events"].as_array().unwrap().len(), 2);
    assert_eq!(without["events"], json!([]));
    assert_eq!(
        without["free_min"].as_i64().unwrap() - with["free_min"].as_i64().unwrap(),
        60
    );

    // Nothing changed (same ctag): no listing or fetching.
    dav.lock().unwrap().requests.clear();
    t.req("POST", "/api/v1/calendar/sync", Some(&anna), None)
        .await;
    let (_, delta, _) = t
        .req(
            "GET",
            &format!("/api/v1/sync?since={rev}"),
            Some(&anna),
            None,
        )
        .await;
    assert_eq!(
        delta["events"].as_array().unwrap().len(),
        0,
        "a forced sync rewrites nothing that didn't change"
    );

    // A task scheduled on top of an event: the next sync tells the user, once.
    let (_, task, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&anna),
            Some(json!({"title": "Gym", "estimate_min": 30})),
        )
        .await;
    t.req(
        "POST",
        &format!("/api/v1/days/{}/entries", tomorrow.format("%Y-%m-%d")),
        Some(&anna),
        Some(json!({"task_id": task["id"], "start_time": tomorrow.format("%H:%M").to_string()})),
    )
    .await;
    let mut rx = t.state.bus.subscribe();
    t.req("POST", "/api/v1/calendar/sync", Some(&anna), None)
        .await;
    t.req("POST", "/api/v1/calendar/sync", Some(&anna), None)
        .await;
    let mut notes = vec![];
    while let Ok(c) = rx.try_recv() {
        if c.kind == "notification" {
            notes.push(c.data.clone());
        }
    }
    // Gym overlaps both 09:00 events; the second sync repeats nothing.
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert!(
        notes
            .iter()
            .all(|n| n["kind"] == "conflict" && n["body"].as_str().unwrap().contains("Gym"))
    );
    t.req(
        "DELETE",
        &format!("/api/v1/tasks/{}", task["id"].as_str().unwrap()),
        Some(&anna),
        None,
    )
    .await;

    // One event edited, the series deleted: only the edited one is fetched.
    {
        let mut d = dav.lock().unwrap();
        d.ctag = 2;
        d.objects.insert(
            "/dav/anna/work/a.ics".into(),
            (
                "\"2\"".into(),
                vevent("a", tomorrow, "SUMMARY:Dentist (moved)\r\n"),
            ),
        );
        d.objects.remove("/dav/anna/work/b.ics");
        d.requests.clear();
    }
    t.req("POST", "/api/v1/calendar/sync", Some(&anna), None)
        .await;
    let multiget = dav
        .lock()
        .unwrap()
        .requests
        .iter()
        .filter(|r| r.contains("multiget"))
        .count();
    assert_eq!(multiget, 1);
    let (_, delta, _) = t
        .req(
            "GET",
            &format!("/api/v1/sync?since={rev}"),
            Some(&anna),
            None,
        )
        .await;
    let ev = delta["events"].as_array().unwrap();
    assert_eq!(ev.len(), 4, "{ev:?}");
    assert_eq!(
        ev.iter().find(|e| e["uid"] == "a").unwrap()["title"],
        "Dentist (moved)"
    );
    assert_eq!(
        ev.iter()
            .filter(|e| e["uid"] == "b" && e["deleted_at"].is_string())
            .count(),
        3
    );

    // Other people see nothing of it.
    let ben = t.user(&admin, "ben").await;
    let (_, b, _) = t.req("GET", "/api/v1/sync", Some(&ben), None).await;
    assert_eq!(b["events"], json!([]));
    assert_eq!(b["calendar_account"], Value::Null);
    let cal_id = full["calendars"][0]["id"].as_str().unwrap().to_string();
    let (s, _, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/calendars/{cal_id}"),
            Some(&ben),
            Some(json!({"enabled": false})),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // A failing server keeps the last known events and shows the error.
    let (_, acc, _) = t
        .req(
            "PUT",
            "/api/v1/calendar/account",
            Some(&anna),
            Some(json!({"url": url, "username": "anna", "password": "nope"})),
        )
        .await;
    assert_eq!(acc["status"], "error");
    assert!(acc["last_error"].as_str().unwrap().contains("rejected"));
    let (_, now_full, _) = t.req("GET", "/api/v1/sync", Some(&anna), None).await;
    assert_eq!(now_full["events"].as_array().unwrap().len(), 1);

    // Back to the right password (leaving it out keeps the stored one).
    t.req(
        "PUT",
        "/api/v1/calendar/account",
        Some(&anna),
        Some(json!({"url": url, "username": "anna", "password": "secret"})),
    )
    .await;
    let (_, acc, _) = t
        .req(
            "PUT",
            "/api/v1/calendar/account",
            Some(&anna),
            Some(json!({"url": url, "username": "anna"})),
        )
        .await;
    assert_eq!(acc["status"], "ok", "{acc}");

    // Hiding a calendar removes its events; disconnecting removes everything.
    let (_, c, _) = t
        .req(
            "PATCH",
            &format!("/api/v1/calendars/{cal_id}"),
            Some(&anna),
            Some(json!({"enabled": false, "user_color": "#ff0000"})),
        )
        .await;
    assert_eq!(c["enabled"], false);
    assert_eq!(c["user_color"], "#ff0000");
    let (_, f, _) = t.req("GET", "/api/v1/sync", Some(&anna), None).await;
    assert_eq!(f["events"], json!([]));
    let (s, _, _) = t
        .req("DELETE", "/api/v1/calendar/account", Some(&anna), None)
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, f, _) = t.req("GET", "/api/v1/sync", Some(&anna), None).await;
    assert_eq!(f["calendar_account"], Value::Null);
    assert_eq!(f["calendars"], json!([]));
}

#[tokio::test]
async fn namedays_people_and_occasion_tasks() {
    let t = setup().await;
    let admin = t.admin().await;
    let anna = t.user(&admin, "anna").await;
    let (_, today, _) = t.req("GET", "/api/v1/today", Some(&anna), None).await;
    let today =
        chrono::NaiveDate::parse_from_str(today["date"].as_str().unwrap(), "%Y-%m-%d").unwrap();
    let md = |days: i64| {
        (today + chrono::Duration::days(days))
            .format("%m-%d")
            .to_string()
    };
    let ymd = |days: i64| {
        (today + chrono::Duration::days(days))
            .format("%Y-%m-%d")
            .to_string()
    };

    // Only admins load the calendar; everyone reads it.
    let list = format!("# test\n{} Testa, Tõnu\n{} Kaarel\n", md(3), md(40));
    let (s, _, _) = t
        .req(
            "POST",
            "/api/v1/namedays",
            Some(&anna),
            Some(json!({"text": list})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, src, _) = t
        .req(
            "POST",
            "/api/v1/namedays",
            Some(&admin),
            Some(json!({"text": list, "label": "Test"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{src}");
    assert_eq!(src["count"], 3);
    let (_, cal, _) = t.req("GET", "/api/v1/namedays", Some(&anna), None).await;
    assert_eq!(cal["source"]["label"], "Test");
    assert_eq!(cal["days"].as_array().unwrap().len(), 2);
    let (_, found, _) = t
        .req("GET", "/api/v1/namedays/search?q=tonu", Some(&anna), None)
        .await;
    assert_eq!(
        found,
        json!([{"name": "Tõnu", "dates": [md(3)]}]),
        "accents are optional"
    );
    let (_, found, _) = t
        .req("GET", "/api/v1/namedays/search?q=a", Some(&anna), None)
        .await;
    assert_eq!(found.as_array().unwrap().len(), 2);

    // A nameday 3 days away: the greeting task exists (it appears a week ahead).
    let (s, mari, _) = t
        .req(
            "POST",
            "/api/v1/people",
            Some(&anna),
            Some(json!({"name": "Mari", "nameday_name": "testa"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{mari}");
    // A birthday in 5 days with a known year: present 2 days before, then the greeting.
    let born = format!("1990-{}", md(5));
    let (s, jaan, _) = t
        .req(
            "POST",
            "/api/v1/people",
            Some(&anna),
            Some(json!({"name": "Jaan", "birthday": born})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{jaan}");
    // Nameday 40 days away: nothing yet.
    t.req(
        "POST",
        "/api/v1/people",
        Some(&anna),
        Some(json!({"name": "Kaarel", "nameday_name": "Kaarel"})),
    )
    .await;

    let occasion_tasks = |v: &Value| -> Vec<Value> {
        v["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["ext_source"] == "occasion" && t["deleted_at"].is_null())
            .cloned()
            .collect()
    };
    let (_, full, _) = t.req("GET", "/api/v1/sync", Some(&anna), None).await;
    let tasks = occasion_tasks(&full);
    let titles: Vec<_> = tasks
        .iter()
        .map(|t| {
            (
                t["title"].as_str().unwrap(),
                t["due_date"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(tasks.len(), 3, "{titles:?}");
    assert!(titles.contains(&("Wish Mari a happy nameday", ymd(3).as_str())));
    let present = tasks
        .iter()
        .find(|t| t["title"] == "Buy a present for Jaan")
        .unwrap();
    let greet = tasks
        .iter()
        .find(|t| t["title"] == "Wish Jaan a happy birthday")
        .unwrap();
    assert_eq!(present["due_date"], ymd(3));
    assert_eq!(present["task_type_id"], "tt_deadline");
    assert_eq!(greet["due_date"], ymd(5));
    assert_eq!(greet["task_type_id"], "tt_expires");
    assert_eq!(greet["blocked"], true, "waits for the present");
    let age = (today + chrono::Duration::days(5))
        .format("%Y")
        .to_string()
        .parse::<i32>()
        .unwrap()
        - 1990;
    assert!(
        greet["notes"]
            .as_str()
            .unwrap()
            .contains(&format!("turns {age}")),
        "{}",
        greet["notes"]
    );
    assert_eq!(full["occasion_templates"].as_array().unwrap().len(), 2);
    assert_eq!(full["people"].as_array().unwrap().len(), 3);

    // Buying the present unblocks the greeting.
    t.req(
        "PATCH",
        &format!("/api/v1/tasks/{}", present["id"].as_str().unwrap()),
        Some(&anna),
        Some(json!({"status": "done"})),
    )
    .await;
    let (_, g, _) = t
        .req(
            "GET",
            &format!("/api/v1/tasks/{}", greet["id"].as_str().unwrap()),
            Some(&anna),
            None,
        )
        .await;
    assert_eq!(g["blocked"], false);

    // No duplicates, even after deleting one.
    t.req(
        "DELETE",
        &format!("/api/v1/tasks/{}", greet["id"].as_str().unwrap()),
        Some(&anna),
        None,
    )
    .await;
    let (_, again, _) = t.req("GET", "/api/v1/sync", Some(&anna), None).await;
    assert_eq!(occasion_tasks(&again).len(), 2);

    // Bad input.
    let (s, _, _) = t
        .req(
            "POST",
            "/api/v1/people",
            Some(&anna),
            Some(json!({"name": "X", "birthday": "31.12"})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _, _) = t
        .req("PUT", "/api/v1/occasion-templates/birthday", Some(&anna), Some(json!({"steps": [{"title": "x", "offset_days": -90, "task_type_id": "tt_expires"}]})))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, tpl, _) = t
        .req(
            "PUT",
            "/api/v1/occasion-templates/nameday",
            Some(&anna),
            Some(json!({"enabled": false})),
        )
        .await;
    assert_eq!((s, &tpl["enabled"]), (StatusCode::OK, &json!(false)));

    // Removing Mari removes her upcoming greeting; other people's lists are their own.
    let (s, _, _) = t
        .req(
            "DELETE",
            &format!("/api/v1/people/{}", mari["id"].as_str().unwrap()),
            Some(&anna),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, after, _) = t.req("GET", "/api/v1/sync", Some(&anna), None).await;
    assert!(
        !occasion_tasks(&after)
            .iter()
            .any(|t| t["title"] == "Wish Mari a happy nameday")
    );
    let ben = t.user(&admin, "ben").await;
    let (_, b, _) = t.req("GET", "/api/v1/sync", Some(&ben), None).await;
    assert_eq!(b["people"], json!([]));
    let (_, bc, _) = t.req("GET", "/api/v1/namedays", Some(&ben), None).await;
    assert_eq!(
        bc["days"].as_array().unwrap().len(),
        2,
        "the calendar is shared"
    );
}

#[tokio::test]
async fn day_templates_blocks_conflicts_and_suggestions() {
    let t = setup().await;
    let admin = t.admin().await;
    let a = t.user(&admin, "anna").await;
    let (_, today, _) = t.req("GET", "/api/v1/today", Some(&a), None).await;
    let today =
        chrono::NaiveDate::parse_from_str(today["date"].as_str().unwrap(), "%Y-%m-%d").unwrap();
    let day = |n: i64| {
        (today + chrono::Duration::days(n))
            .format("%Y-%m-%d")
            .to_string()
    };

    let blocks = json!([
        {"title": "Deep work", "start": "08:00", "end": "12:00", "energy": "hard"},
        {"title": "Admin", "start": "13:00", "end": "15:00", "energy": "easy"}
    ]);
    let (s, _, _) = t
        .req("POST", "/api/v1/day-templates", Some(&a), Some(json!({"name": "Bad", "blocks": [{"title": "x", "start": "12:00", "end": "11:00", "energy": null}]})))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, work, _) = t
        .req(
            "POST",
            "/api/v1/day-templates",
            Some(&a),
            Some(json!({"name": "Workday", "weekdays": [1, 2, 3, 4, 5, 6, 7], "blocks": blocks})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{work}");
    // A weekend template takes Saturday and Sunday away from "Workday".
    let (_, weekend, _) = t
        .req("POST", "/api/v1/day-templates", Some(&a), Some(json!({"name": "Weekend", "weekdays": [6, 7], "blocks": [{"title": "Chores", "start": "10:00", "end": "12:00", "energy": null}]})))
        .await;
    let (_, full, _) = t.req("GET", "/api/v1/sync", Some(&a), None).await;
    let tpls = full["day_templates"].as_array().unwrap();
    assert_eq!(
        tpls.iter().find(|x| x["id"] == work["id"]).unwrap()["weekdays"],
        json!([1, 2, 3, 4, 5])
    );
    let blocks_on = |v: &Value, d: &str| -> Vec<Value> {
        v["time_blocks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| b["date"] == d && b["deleted_at"].is_null())
            .cloned()
            .collect()
    };
    // The next 8 days got their weekday's blocks.
    for n in 0..8 {
        let d = today + chrono::Duration::days(n);
        let expected = if chrono::Datelike::weekday(&d).number_from_monday() >= 6 {
            1
        } else {
            2
        };
        assert_eq!(blocks_on(&full, &day(n)).len(), expected, "day {n}");
    }
    let _ = weekend;

    // Edits stick: a deleted block doesn't come back.
    let first = blocks_on(&full, &day(1))[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    t.req(
        "DELETE",
        &format!("/api/v1/time-blocks/{first}"),
        Some(&a),
        None,
    )
    .await;
    let (_, again, _) = t.req("GET", "/api/v1/sync", Some(&a), None).await;
    assert_eq!(
        blocks_on(&again, &day(1)).len(),
        blocks_on(&full, &day(1)).len() - 1
    );

    // Applying: clear, re-apply, and applying the same template again changes nothing.
    let d3 = day(3);
    let (_, none, _) = t
        .req(
            "POST",
            &format!("/api/v1/days/{d3}/apply-template"),
            Some(&a),
            Some(json!({"template_id": null})),
        )
        .await;
    assert_eq!(none, json!([]));
    let (_, b1, _) = t
        .req(
            "POST",
            &format!("/api/v1/days/{d3}/apply-template"),
            Some(&a),
            Some(json!({"template_id": work["id"]})),
        )
        .await;
    let (_, b2, _) = t
        .req(
            "POST",
            &format!("/api/v1/days/{d3}/apply-template"),
            Some(&a),
            Some(json!({"template_id": work["id"]})),
        )
        .await;
    assert_eq!(b1.as_array().unwrap().len(), 2);
    assert_eq!(b1, b2, "idempotent");

    // Conflicts: two tasks overlapping each other on day 3; a task inside a block is fine.
    let mk = |title: &str, extra: Value| {
        let mut body = json!({"title": title});
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        body
    };
    let (_, ta, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&a),
            Some(mk("Write", json!({"estimate_min": 60}))),
        )
        .await;
    let (_, tb, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&a),
            Some(mk("Call", json!({}))),
        )
        .await;
    t.req(
        "POST",
        &format!("/api/v1/days/{d3}/entries"),
        Some(&a),
        Some(json!({"task_id": ta["id"], "start_time": "09:00"})),
    )
    .await;
    let (_, eb, _) = t
        .req(
            "POST",
            &format!("/api/v1/days/{d3}/entries"),
            Some(&a),
            Some(json!({"task_id": tb["id"], "start_time": "09:30", "duration_min": 30})),
        )
        .await;
    let (_, dv, _) = t
        .req("GET", &format!("/api/v1/days/{d3}"), Some(&a), None)
        .await;
    assert_eq!(dv["blocks"].as_array().unwrap().len(), 2);
    let c = dv["conflicts"].as_array().unwrap();
    assert_eq!(c.len(), 1, "{c:?}");
    assert_eq!(
        (
            c[0]["a_kind"].as_str(),
            c[0]["b_kind"].as_str(),
            c[0]["minutes"].as_i64()
        ),
        (Some("task"), Some("task"), Some(30))
    );
    assert_eq!(c[0]["b"], eb["id"]);

    // Suggest times for the unscheduled planned tasks: hard → deep work, easy → admin.
    let (_, hard, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&a),
            Some(mk("Design", json!({"difficulty": 3, "estimate_min": 60}))),
        )
        .await;
    let (_, easy, _) = t
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&a),
            Some(mk("Email", json!({"difficulty": 1, "estimate_min": 30}))),
        )
        .await;
    for id in [&hard["id"], &easy["id"]] {
        t.req(
            "POST",
            &format!("/api/v1/days/{d3}/entries"),
            Some(&a),
            Some(json!({"task_id": id})),
        )
        .await;
    }
    let (s, plan, _) = t
        .req(
            "POST",
            &format!("/api/v1/days/{d3}/suggest"),
            Some(&a),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{plan}");
    let sug = plan["suggestions"].as_array().unwrap();
    let of = |id: &Value| sug.iter().find(|s| &s["task_id"] == id).unwrap().clone();
    // 08:00 is free (Write/Call are 09:00–10:00): Design fits 08:00–09:00.
    assert_eq!(of(&hard["id"])["start_time"], "08:00");
    assert!(
        of(&hard["id"])["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["code"] == "hard_in_hard_block")
    );
    assert_eq!(of(&easy["id"])["start_time"], "13:00");
    assert_eq!(plan["unplaced"], json!([]));
    // Nothing was saved.
    let (_, dv2, _) = t
        .req("GET", &format!("/api/v1/days/{d3}"), Some(&a), None)
        .await;
    assert_eq!(
        dv2["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["start_time"].is_null())
            .count(),
        2
    );
}
