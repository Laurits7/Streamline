//! Sharing projects between two Streamline servers (Phase 8): both run in this process,
//! the home server on a real port (the replica calls it over HTTP).

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
    ) -> (StatusCode, Value) {
        let mut b = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "localhost:3000");
        if let Some(a) = auth {
            b = if a.starts_with("sl") && a.contains('_') && !a.contains('=') && a.len() > 40 {
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
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn login(&self, name: &str) -> String {
        let mut b = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/login")
            .header(header::HOST, "localhost:3000")
            .header(header::CONTENT_TYPE, "application/json");
        b = b.header(header::ORIGIN, "http://localhost:3000");
        let mut req = b
            .body(Body::from(
                json!({"username": name, "password": "password123"}).to_string(),
            ))
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 9999))));
        let res = self.app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        res.headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|c| c.split(';').next())
            .and_then(|kv| kv.strip_prefix("sl_session="))
            .unwrap()
            .to_string()
    }

    async fn admin(&self) -> String {
        let (s, _) = self
            .req(
                "POST",
                "/api/v1/setup",
                None,
                Some(json!({"username": "admin", "password": "password123"})),
            )
            .await;
        assert_eq!(s, StatusCode::OK);
        self.login("admin").await
    }

    async fn user(&self, admin: &str, name: &str) -> (String, String) {
        let (s, v) = self
            .req(
                "POST",
                "/api/v1/users",
                Some(admin),
                Some(json!({"username": name, "password": "password123"})),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{v}");
        let session = self.login(name).await;
        let (_, me) = self.req("GET", "/api/v1/me", Some(&session), None).await;
        (session, me["id"].as_str().unwrap().to_string())
    }

    async fn sync(&self, auth: &str) -> Value {
        let (s, v) = self.req("GET", "/api/v1/sync", Some(auth), None).await;
        assert_eq!(s, StatusCode::OK);
        v
    }
}

/// Serve the app on a real port; dropping the sender stops it.
async fn serve(t: &T) -> (String, tokio::sync::oneshot::Sender<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = t.app.clone();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async {
            let _ = stopped.await;
        })
        .await
        .unwrap()
    });
    (format!("http://{addr}"), stop)
}

fn find<'a>(list: &'a Value, id: &str) -> Option<&'a Value> {
    list.as_array()?.iter().find(|x| x["id"] == id)
}

fn live<'a>(list: &'a Value, id: &str) -> Option<&'a Value> {
    find(list, id).filter(|x| x["deleted_at"].is_null())
}

#[tokio::test]
async fn share_a_project_with_another_server() {
    // ---- Home server: Alice shares "Garden" (with a subproject, tasks, a routine) with
    // her group "Family".
    let h = setup().await;
    let (home_url, stop_home) = serve(&h).await;
    let h_admin = h.admin().await;
    let (alice, _alice_id) = h.user(&h_admin, "alice").await;
    let (hugo, hugo_id) = h.user(&h_admin, "hugo").await;
    let (_, g) = h
        .req(
            "POST",
            "/api/v1/groups",
            Some(&alice),
            Some(json!({"name": "Family"})),
        )
        .await;
    let group = g["id"].as_str().unwrap().to_string();
    h.req(
        "POST",
        &format!("/api/v1/groups/{group}/members"),
        Some(&alice),
        Some(json!({"user_id": hugo_id})),
    )
    .await;
    let (_, p) = h
        .req(
            "POST",
            "/api/v1/projects",
            Some(&alice),
            Some(json!({"name": "Garden", "owner_group_id": group})),
        )
        .await;
    let garden = p["id"].as_str().unwrap().to_string();
    let (_, p) = h
        .req(
            "POST",
            "/api/v1/projects",
            Some(&alice),
            Some(json!({"name": "Beds", "parent_id": garden})),
        )
        .await;
    let beds = p["id"].as_str().unwrap().to_string();
    let (_, t1) = h
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&alice),
            Some(json!({"title": "Buy seeds", "project_id": garden})),
        )
        .await;
    let seeds = t1["id"].as_str().unwrap().to_string();
    let (_, t2) = h
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&alice),
            Some(json!({"title": "Dig the bed", "project_id": beds})),
        )
        .await;
    let dig = t2["id"].as_str().unwrap().to_string();
    let (s, r) = h
        .req("POST", "/api/v1/series", Some(&alice), Some(json!({"title": "Water", "mode": "repeat", "rrule": "FREQ=DAILY", "project_id": garden})))
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    let water = r["id"].as_str().unwrap().to_string();
    let (_, own) = h
        .req(
            "POST",
            "/api/v1/projects",
            Some(&alice),
            Some(json!({"name": "Alice's own"})),
        )
        .await;
    let alices_own = own["id"].as_str().unwrap().to_string();

    // Only the group's owners can invite, only to a top-level group project.
    let (s, _) = h
        .req(
            "POST",
            &format!("/api/v1/projects/{garden}/invites"),
            Some(&hugo),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = h
        .req(
            "POST",
            &format!("/api/v1/projects/{beds}/invites"),
            Some(&alice),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = h
        .req(
            "POST",
            &format!("/api/v1/projects/{alices_own}/invites"),
            Some(&alice),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, inv) = h
        .req(
            "POST",
            &format!("/api/v1/projects/{garden}/invites"),
            Some(&alice),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{inv}");
    let invite = format!("{home_url}/join/{}", inv["token"].as_str().unwrap());

    // ---- Replica server: Anna joins with the link.
    let r = setup().await;
    let r_admin = r.admin().await;
    let (anna, anna_id) = r.user(&r_admin, "anna").await;
    let (carl, _) = r.user(&r_admin, "carl").await;
    let (s, j) = r
        .req(
            "POST",
            "/api/v1/federation/joins",
            Some(&anna),
            Some(json!({"invite": invite, "origin": "http://r.test"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{j}");
    assert_eq!(j["status"], "ok", "{j}");
    let join_id = j["id"].as_str().unwrap().to_string();
    // An invite works once.
    let (s, v) = r
        .req(
            "POST",
            "/api/v1/federation/joins",
            Some(&carl),
            Some(json!({"invite": invite, "origin": "http://r.test"})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");

    // Anna sees the project, its subproject, tasks and routine; Carl sees none of it.
    let v = r.sync(&anna).await;
    assert!(live(&v["projects"], &garden).is_some());
    assert!(live(&v["projects"], &beds).is_some());
    assert!(live(&v["tasks"], &seeds).is_some());
    assert!(live(&v["tasks"], &dig).is_some());
    assert!(live(&v["series"], &water).is_some());
    assert!(find(&v["projects"], &alices_own).is_none());
    assert_eq!(v["remote_projects"][0]["root_project_id"], garden);
    let (_, groups) = r.req("GET", "/api/v1/groups", Some(&anna), None).await;
    let mirror = find(&groups, &group).expect("mirrored group");
    assert_eq!(mirror["home"], home_url.as_str());
    assert!(
        mirror["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["username"].as_str().unwrap().starts_with("alice@"))
    );
    let v = r.sync(&carl).await;
    assert!(find(&v["projects"], &garden).is_none());
    assert!(find(&v["tasks"], &seeds).is_none());
    // At home, Anna is in the group as someone from r.test.
    let (_, g) = h.req("GET", "/api/v1/groups", Some(&alice), None).await;
    let fam = find(&g, &group).unwrap();
    assert!(
        fam["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["user_id"] == anna_id && m["username"] == "anna@r.test")
    );
    let (_, members) = h
        .req(
            "GET",
            &format!("/api/v1/projects/{garden}/remote-members"),
            Some(&alice),
            None,
        )
        .await;
    assert_eq!(members[0]["username"], "anna@r.test");
    // People from elsewhere can't sign in, and don't show in the user list or directory.
    let (_, users) = h.req("GET", "/api/v1/users", Some(&h_admin), None).await;
    assert!(find(&users, &anna_id).is_none());
    let (_, dir) = h
        .req("GET", "/api/v1/users/directory", Some(&alice), None)
        .await;
    assert!(find(&dir, &anna_id).is_none());

    // ---- Edits on the replica go home. Completing a task:
    let (s, v) = r
        .req(
            "PATCH",
            &format!("/api/v1/tasks/{seeds}"),
            Some(&anna),
            Some(json!({"status": "done"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["status"], "done");
    let (_, home_task) = h
        .req("GET", &format!("/api/v1/tasks/{seeds}"), Some(&alice), None)
        .await;
    assert_eq!(home_task["status"], "done");
    assert_eq!(home_task["completed_by"], anna_id);
    let local = live(&r.sync(&anna).await["tasks"], &seeds).unwrap().clone();
    assert_eq!(local["status"], "done");
    assert_eq!(
        v["rev"], local["rev"],
        "answers carry this server's revisions"
    );
    // Creating a task in the shared project (the client's id is kept):
    let new_id = ulid::Ulid::new().to_string();
    let (s, v) = r
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&anna),
            Some(json!({"id": new_id, "title": "Mulch", "project_id": beds})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (s, home_task) = h
        .req(
            "GET",
            &format!("/api/v1/tasks/{new_id}"),
            Some(&alice),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(home_task["title"], "Mulch");
    assert!(live(&r.sync(&anna).await["tasks"], &new_id).is_some());
    // Planning a shared task on her own day stays on her server.
    let today = r.sync(&anna).await["today"].as_str().unwrap().to_string();
    let (s, v) = r
        .req(
            "POST",
            &format!("/api/v1/days/{today}/entries"),
            Some(&anna),
            Some(json!({"task_id": dig})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");

    // Things outside the project are refused: moving a task out, into a local project,
    // or touching a project item the link doesn't cover.
    let (_, mine) = r
        .req(
            "POST",
            "/api/v1/projects",
            Some(&anna),
            Some(json!({"name": "Anna's"})),
        )
        .await;
    let annas = mine["id"].as_str().unwrap().to_string();
    let (s, _) = r
        .req(
            "PATCH",
            &format!("/api/v1/tasks/{dig}"),
            Some(&anna),
            Some(json!({"project_id": annas})),
        )
        .await;
    assert!(s.is_client_error(), "{s}");
    let (s, _) = r
        .req(
            "PATCH",
            &format!("/api/v1/tasks/{dig}"),
            Some(&anna),
            Some(json!({"project_id": null})),
        )
        .await;
    assert!(s.is_client_error(), "{s}");
    let (_, local_task) = r
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&anna),
            Some(json!({"title": "Mine", "project_id": annas})),
        )
        .await;
    let (s, _) = r
        .req(
            "PATCH",
            &format!("/api/v1/tasks/{}", local_task["id"].as_str().unwrap()),
            Some(&anna),
            Some(json!({"project_id": garden})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = r
        .req(
            "DELETE",
            &format!("/api/v1/projects/{garden}"),
            Some(&anna),
            None,
        )
        .await;
    assert!(s.is_client_error(), "{s}");
    // The link's key can't reach the rest of the home server either.
    let secret: Vec<u8> = sqlx::query_scalar("SELECT secret FROM fed_joins WHERE id = ?")
        .bind(&join_id)
        .fetch_one(&r.state.db.read)
        .await
        .unwrap();
    let secret = r.state.secrets.decrypt(&secret).unwrap();
    let (_, other) = h
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&alice),
            Some(json!({"title": "Private", "project_id": alices_own})),
        )
        .await;
    let (s, v) = h
        .req(
            "POST",
            &format!("/api/v1/federation/links/{join_id}/forward"),
            Some(&secret),
            Some(json!({"method": "PATCH", "path": format!("/tasks/{}", other["id"].as_str().unwrap()), "body": {"title": "x"}})),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{v}");
    let (s, _) = h
        .req(
            "POST",
            &format!("/api/v1/federation/links/{join_id}/forward"),
            Some(&secret),
            Some(json!({"method": "POST", "path": "/groups", "body": {"name": "x"}})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = h
        .req(
            "GET",
            &format!("/api/v1/federation/links/{join_id}/feed"),
            Some("sls_wrongwrongwrongwrongwrongwrongwrongwrongwrong"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    // The mirrored group is managed at home.
    let (s, _) = r
        .req(
            "PATCH",
            &format!("/api/v1/groups/{group}"),
            Some(&r_admin),
            Some(json!({"name": "x"})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = r
        .req(
            "POST",
            "/api/v1/projects",
            Some(&anna),
            Some(json!({"name": "Sneaky", "owner_group_id": group})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // ---- Edits at home reach the replica (here pulled directly; the live stream pokes
    // in real use). A task moved out of the project disappears from the copy.
    h.req(
        "PATCH",
        &format!("/api/v1/tasks/{dig}"),
        Some(&hugo),
        Some(json!({"title": "Dig the big bed"})),
    )
    .await;
    h.req(
        "PATCH",
        &format!("/api/v1/tasks/{new_id}"),
        Some(&alice),
        Some(json!({"project_id": alices_own})),
    )
    .await;
    streamline::federation::pull(&r.state, &join_id)
        .await
        .unwrap();
    let v = r.sync(&anna).await;
    assert_eq!(live(&v["tasks"], &dig).unwrap()["title"], "Dig the big bed");
    assert!(live(&v["tasks"], &new_id).is_none());
    // Routines are made only at home: the replica doesn't spawn its own occurrences.
    let count = |v: &Value| {
        v["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["series_id"] == water && t["deleted_at"].is_null())
            .count()
    };
    let before = count(&r.sync(&anna).await);
    streamline::routines::materialize_all(&r.state)
        .await
        .unwrap();
    assert_eq!(count(&r.sync(&anna).await), before);

    // ---- Leaving: the copy goes here and Anna leaves the group at home.
    let (s, _) = r
        .req(
            "DELETE",
            &format!("/api/v1/federation/joins/{join_id}"),
            Some(&anna),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let v = r.sync(&anna).await;
    assert!(live(&v["projects"], &garden).is_none());
    assert!(live(&v["tasks"], &dig).is_none());
    assert_eq!(v["remote_projects"], json!([]));
    let (_, g) = h.req("GET", "/api/v1/groups", Some(&alice), None).await;
    assert!(
        !find(&g, &group).unwrap()["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["user_id"] == anna_id)
    );

    // ---- Joining again; then the home server removes her: the copy goes on the next pull.
    let (_, inv) = h
        .req(
            "POST",
            &format!("/api/v1/projects/{garden}/invites"),
            Some(&alice),
            None,
        )
        .await;
    let (s, j) = r
        .req("POST", "/api/v1/federation/joins", Some(&anna), Some(json!({"invite": format!("{home_url}/join/{}", inv["token"].as_str().unwrap()), "origin": "http://r.test"})))
        .await;
    assert_eq!(s, StatusCode::OK, "{j}");
    let join_id = j["id"].as_str().unwrap().to_string();
    assert!(live(&r.sync(&anna).await["tasks"], &dig).is_some());
    let (_, members) = h
        .req(
            "GET",
            &format!("/api/v1/projects/{garden}/remote-members"),
            Some(&alice),
            None,
        )
        .await;
    let link = members[0]["id"].as_str().unwrap().to_string();
    let (s, _) = h
        .req(
            "DELETE",
            &format!("/api/v1/federation/links/{link}"),
            Some(&hugo),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "only owners remove people");

    // ---- Home server down: the copy stays, edits are refused with a clear message.
    drop(stop_home);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let (s, v) = r
        .req(
            "PATCH",
            &format!("/api/v1/tasks/{dig}"),
            Some(&anna),
            Some(json!({"title": "offline"})),
        )
        .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE, "{v}");
    assert!(v["detail"].as_str().unwrap().contains("read-only"));
    assert!(
        streamline::federation::pull(&r.state, &join_id)
            .await
            .is_err()
    );
    let v = r.sync(&anna).await;
    assert_eq!(live(&v["tasks"], &dig).unwrap()["title"], "Dig the big bed");
    assert_eq!(v["remote_projects"][0]["status"], "error");

    // Back up, Alice removes Anna: the next pull drops the copy.
    let (home_url2, _stop) = serve(&h).await;
    sqlx::query("UPDATE fed_joins SET home_url = ?")
        .bind(&home_url2)
        .execute(&r.state.db.write)
        .await
        .unwrap();
    let (s, _) = h
        .req(
            "DELETE",
            &format!("/api/v1/federation/links/{link}"),
            Some(&alice),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    streamline::federation::pull(&r.state, &join_id)
        .await
        .unwrap();
    let v = r.sync(&anna).await;
    assert!(live(&v["projects"], &garden).is_none());
    assert_eq!(v["remote_projects"], json!([]));
    let (_, groups) = r.req("GET", "/api/v1/groups", Some(&anna), None).await;
    assert!(find(&groups, &group).is_none());
}

#[tokio::test]
async fn live_stream_brings_home_edits_over() {
    let h = setup().await;
    let (home_url, _stop) = serve(&h).await;
    let h_admin = h.admin().await;
    let (alice, _) = h.user(&h_admin, "alice").await;
    let (_, g) = h
        .req(
            "POST",
            "/api/v1/groups",
            Some(&alice),
            Some(json!({"name": "Family"})),
        )
        .await;
    let (_, p) = h
        .req(
            "POST",
            "/api/v1/projects",
            Some(&alice),
            Some(json!({"name": "Garden", "owner_group_id": g["id"]})),
        )
        .await;
    let garden = p["id"].as_str().unwrap().to_string();
    let (_, inv) = h
        .req(
            "POST",
            &format!("/api/v1/projects/{garden}/invites"),
            Some(&alice),
            None,
        )
        .await;

    let r = setup().await;
    let r_admin = r.admin().await;
    let (anna, _) = r.user(&r_admin, "anna").await;
    let (s, j) = r
        .req("POST", "/api/v1/federation/joins", Some(&anna), Some(json!({"invite": format!("{home_url}/join/{}", inv["token"].as_str().unwrap()), "origin": "http://r.test"})))
        .await;
    assert_eq!(s, StatusCode::OK, "{j}");
    // The background round starts a listener on the home server's stream.
    streamline::federation::run_due(&r.state).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let (_, t) = h
        .req(
            "POST",
            "/api/v1/tasks",
            Some(&alice),
            Some(json!({"title": "Plant tulips", "project_id": garden})),
        )
        .await;
    let id = t["id"].as_str().unwrap().to_string();
    for _ in 0..50 {
        if live(&r.sync(&anna).await["tasks"], &id).is_some() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("the new task didn't reach the replica");
}
