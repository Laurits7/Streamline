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
    sqlx::query("UPDATE tasks SET occurrence_date = ? WHERE id = ?")
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
