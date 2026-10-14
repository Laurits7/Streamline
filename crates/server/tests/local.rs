//! The server inside the desktop app (D-88).

use serde_json::Value;

#[tokio::test]
async fn local_server_starts_and_signs_the_app_in() {
    let dir = std::env::temp_dir().join(format!("streamline-local-{}", ulid::Ulid::new()));
    let _ = rustls::crypto::ring::default_provider().install_default();
    let http = reqwest::Client::new();
    let get = |url: String, token: String| {
        let http = http.clone();
        async move {
            let res = http.get(url).bearer_auth(token).send().await.unwrap();
            let bytes = res.bytes().await.unwrap();
            serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null)
        }
    };
    let first = streamline::local::start(dir.clone(), "Anna").await.unwrap();
    assert!(first.url.starts_with("http://127.0.0.1:"));
    let me = get(format!("{}/api/v1/me", first.url), first.token.clone()).await;
    assert_eq!(
        (
            me["username"].as_str(),
            me["display_name"].as_str(),
            me["is_admin"].as_bool()
        ),
        (Some("me"), Some("Anna"), Some(true))
    );
    // First-run setup is closed from the moment it listens (another local account can't claim it).
    let res = http
        .post(format!("{}/api/v1/setup", first.url))
        .header("content-type", "application/json")
        .body(r#"{"username":"intruder","password":"intruder pw"}"#)
        .send()
        .await
        .unwrap();
    assert_ne!(res.status(), 200);

    // A later start keeps the user and data, and replaces the app's token.
    let second = streamline::local::start(dir.clone(), "ignored")
        .await
        .unwrap();
    assert_ne!(first.url, second.url);
    let me = get(format!("{}/api/v1/me", second.url), second.token.clone()).await;
    assert_eq!(me["display_name"], "Anna");
    let tokens = get(
        format!("{}/api/v1/tokens", second.url),
        second.token.clone(),
    )
    .await;
    assert_eq!(tokens.as_array().map(Vec::len), Some(1), "{tokens}");
    let old = get(format!("{}/api/v1/me", second.url), first.token).await;
    assert_eq!(old["status"], 401);
    // Its folder is a normal server data folder; with a password set, the account can sign in there.
    assert!(dir.join("streamline.db").exists());
    assert!(
        streamline::local::set_password(&second.state, "short")
            .await
            .is_err()
    );
    streamline::local::set_password(&second.state, "laptop days")
        .await
        .unwrap();
    let res = http
        .post(format!("{}/api/v1/auth/login", second.url))
        .header("content-type", "application/json")
        .body(serde_json::json!({"username": "me", "password": "laptop days"}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}
