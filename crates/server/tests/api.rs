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
