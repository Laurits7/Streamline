//! Streamline for the Linux desktop (D-78): the web app in a window, connected to the
//! user's own Streamline server with a per-device token kept in the system keyring. The
//! page reaches the server only through the commands here. It stays in the tray when the
//! window is closed and can start at login.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine;
mod remote;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

use engine::Engine;

const KEYRING_SERVICE: &str = "streamline-desktop";

/// What's kept on disk (the token itself is in the keyring).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Config {
    server: Option<String>,
    token_id: Option<String>,
    username: Option<String>,
}

#[derive(Default)]
struct App {
    config: Mutex<Config>,
    token: Mutex<Option<String>>,
    events: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    engine: OnceLock<Arc<Engine>>,
}

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("config.json"))
}

fn save_config(app: &AppHandle, c: &Config) -> Result<(), String> {
    let p = config_path(app)?;
    std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&p, serde_json::to_vec_pretty(c).unwrap()).map_err(|e| e.to_string())
}

async fn keyring(
    server: String,
    f: impl FnOnce(keyring::Entry) -> keyring::Result<Option<String>> + Send + 'static,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let entry = keyring::Entry::new(KEYRING_SERVICE, &server)?;
        f(entry)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| match e {
        keyring::Error::PlatformFailure(_) | keyring::Error::NoStorageAccess(_) => {
            "No system keyring is available (GNOME Keyring or KWallet is needed to keep the sign-in safe)".to_string()
        }
        e => e.to_string(),
    })
}

#[derive(Serialize)]
struct Status {
    server: Option<String>,
    username: Option<String>,
    signed_in: bool,
}

fn status_of(state: &App) -> Status {
    let c = state.config.lock().unwrap().clone();
    Status {
        server: c.server,
        username: c.username,
        signed_in: state.token.lock().unwrap().is_some(),
    }
}

#[tauri::command]
fn status(state: State<'_, App>) -> Status {
    status_of(&state)
}

fn device_name() -> String {
    let host = std::fs::read_to_string("/etc/hostname").unwrap_or_default();
    let host = host.trim();
    if host.is_empty() {
        "Desktop app".into()
    } else {
        format!("Desktop app ({host})")
    }
}

/// First run (and signing in again): sign in once with the password to create a device
/// token, then forget the password and the session.
#[tauri::command]
async fn connect(
    app: AppHandle,
    state: State<'_, App>,
    server: String,
    username: String,
    password: String,
) -> Result<Status, String> {
    let server = remote::normalize_server(&server)?;
    let http = remote::client(Some(Duration::from_secs(20)));
    let unreachable = |e: reqwest::Error| format!("Can't reach {server}: {}", e.without_url());
    let res = http
        .post(remote::api_url(&server, "/auth/login")?)
        .json(&json!({"username": username, "password": password}))
        .send()
        .await
        .map_err(unreachable)?;
    match res.status().as_u16() {
        200 => {}
        401 | 403 => return Err("Wrong user name or password".into()),
        404 => return Err(format!("{server} doesn't look like a Streamline server")),
        429 => return Err("Too many attempts; wait a minute and try again".into()),
        s => return Err(format!("The server answered {s}")),
    }
    let cookie = res
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with("sl_session="))
        .and_then(|v| v.split(';').next())
        .ok_or("The server didn't start a session")?
        .to_string();
    let created: Value = http
        .post(remote::api_url(&server, "/tokens")?)
        .header(reqwest::header::COOKIE, &cookie)
        .json(&json!({"name": device_name()}))
        .send()
        .await
        .map_err(unreachable)?
        .error_for_status()
        .map_err(|e| format!("Couldn't create a device token: {}", e.without_url()))?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let _ = http
        .post(remote::api_url(&server, "/auth/logout")?)
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await;
    let token = created["token"]
        .as_str()
        .ok_or("No token in the answer")?
        .to_string();
    let token_id = created["info"]["id"].as_str().map(String::from);

    let t = token.clone();
    keyring(server.clone(), move |e| e.set_password(&t).map(|_| None)).await?;
    // A replica of another server or account isn't kept.
    let engine = engine(&app);
    engine.reset_for(&server).await?;
    engine.set_credentials(Some((server.clone(), token.clone())));
    let config = Config {
        server: Some(server),
        token_id,
        username: Some(username),
    };
    save_config(&app, &config)?;
    *state.config.lock().unwrap() = config;
    *state.token.lock().unwrap() = Some(token);
    Ok(status_of(&state))
}

/// Sign out: revoke the device token on the server (if it can be reached), forget it here.
#[tauri::command]
async fn sign_out(app: AppHandle, state: State<'_, App>) -> Result<(), String> {
    if let Some(h) = state.events.lock().unwrap().take() {
        h.abort();
    }
    let c = state.config.lock().unwrap().clone();
    let token = state.token.lock().unwrap().take();
    if let (Some(server), Some(token), Some(id)) = (&c.server, &token, &c.token_id) {
        let _ = remote::client(Some(Duration::from_secs(10)))
            .delete(remote::api_url(server, &format!("/tokens/{id}"))?)
            .bearer_auth(token)
            .send()
            .await;
    }
    if let Some(server) = c.server.clone() {
        let _ = keyring(server, |e| match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e),
        })
        .await;
    }
    // This computer keeps nothing of the account once signed out.
    let engine = engine(&app);
    engine.set_credentials(None);
    engine.clear().await?;
    let config = Config {
        server: c.server,
        ..Default::default()
    };
    save_config(&app, &config)?;
    *state.config.lock().unwrap() = config;
    Ok(())
}

fn credentials(state: &App) -> Option<(String, String)> {
    let server = state.config.lock().unwrap().server.clone()?;
    let token = state.token.lock().unwrap().clone()?;
    Some((server, token))
}

fn engine(app: &AppHandle) -> Arc<Engine> {
    app.state::<App>()
        .engine
        .get()
        .expect("engine opened at start")
        .clone()
}

/// A request from the page to the server's `/api/v1{path}`, through the local replica
/// and outbox (`touched`: the records the page changed with it, for working offline).
/// Status 0 means the server couldn't be reached and the request couldn't wait.
#[tauri::command]
async fn api(
    app: AppHandle,
    method: String,
    path: String,
    body: Option<Value>,
    touched: Option<Vec<engine::Touched>>,
) -> Result<engine::ApiResponse, String> {
    if !matches!(method.as_str(), "GET" | "POST" | "PUT" | "PATCH" | "DELETE") {
        return Err("refused method".into());
    }
    remote::check_path(&path)?;
    Ok(engine(&app)
        .request(&method, &path, body, touched.unwrap_or_default())
        .await)
}

/// How many changes are waiting to be sent.
#[tauri::command]
async fn outbox_status(app: AppHandle) -> engine::Report {
    engine::Report {
        pending: engine(&app).pending().await,
        ..Default::default()
    }
}

/// Save a download (export, CSV, backup) into the user's Downloads folder.
#[tauri::command]
async fn download(
    app: AppHandle,
    state: State<'_, App>,
    path: String,
    filename: String,
) -> Result<String, String> {
    let (server, token) = credentials(&state).ok_or("Not signed in")?;
    let res = remote::client(Some(Duration::from_secs(300)))
        .get(remote::api_url(&server, &path)?)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?
        .error_for_status()
        .map_err(|e| e.without_url().to_string())?;
    let bytes = remote::read_limited(res).await?;
    let name: String = filename
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    let name = if name.is_empty() || name.starts_with('.') {
        "streamline-download".to_string()
    } else {
        name
    };
    let dir = app.path().download_dir().map_err(|e| e.to_string())?;
    let mut target = dir.join(&name);
    let mut n = 1;
    while target.exists() {
        target = dir.join(format!("{n}-{name}"));
        n += 1;
    }
    std::fs::write(&target, bytes).map_err(|e| e.to_string())?;
    Ok(target.display().to_string())
}

/// The page subscribes to live events (like opening an `EventSource`): a fresh stream, so
/// the server's `hello` reaches it and it catches up.
#[tauri::command]
fn events_open(app: AppHandle) {
    start_events(&app);
}

#[tauri::command]
fn events_close(state: State<'_, App>) {
    if let Some(h) = state.events.lock().unwrap().take() {
        h.abort();
    }
}

/// Follow the server's live events and pass them to the page (`server-event`), with
/// `server-stream` telling whether the stream is up. Reconnects with a growing pause.
fn start_events(app: &AppHandle) {
    let state = app.state::<App>();
    if let Some(h) = state.events.lock().unwrap().take() {
        h.abort();
    }
    let app2 = app.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let mut wait = 1u64;
        loop {
            let Some((server, token)) = credentials(&app2.state::<App>()) else {
                return;
            };
            let Ok(url) = remote::api_url(&server, "/events") else {
                return;
            };
            if let Ok(res) = remote::client(None)
                .get(url)
                .bearer_auth(&token)
                .send()
                .await
            {
                if res.status().as_u16() == 401 {
                    let _ = app2.emit(
                        "server-stream",
                        json!({"open": false, "unauthorized": true}),
                    );
                    return;
                }
                if res.status().is_success() {
                    let mut parser = remote::SseParser::default();
                    let mut stream = res.bytes_stream();
                    let mut opened = false;
                    while let Ok(Some(Ok(chunk))) =
                        tokio::time::timeout(Duration::from_secs(90), stream.next()).await
                    {
                        if !opened {
                            opened = true;
                            wait = 1;
                            let _ = app2.emit("server-stream", json!({"open": true}));
                            // The server is back: send what waited.
                            engine(&app2).kick();
                        }
                        for (event, data) in parser.feed(&String::from_utf8_lossy(&chunk)) {
                            if event == "change" {
                                engine(&app2).ingest_event(&data).await;
                            }
                            let _ =
                                app2.emit("server-event", json!({"event": event, "data": data}));
                        }
                    }
                }
            }
            let _ = app2.emit("server-stream", json!({"open": false}));
            tokio::time::sleep(Duration::from_secs(wait)).await;
            wait = (wait * 2).min(60);
        }
    });
    *state.events.lock().unwrap() = Some(handle);
}

fn show(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn main() {
    let minimized = std::env::args().any(|a| a == "--minimized");
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show(app)
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .manage(App::default())
        .invoke_handler(tauri::generate_handler![
            status,
            connect,
            sign_out,
            api,
            outbox_status,
            download,
            events_open,
            events_close
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            // The replica and outbox (9.5); reports go to the page as `outbox` events.
            let data_dir = app.path().app_data_dir()?;
            let emitter = handle.clone();
            let engine = tauri::async_runtime::block_on(Engine::open(
                &data_dir.join("replica.sqlite"),
                move |r| {
                    let _ = emitter.emit("outbox", r);
                },
            ))?;
            app.state::<App>().engine.set(engine.clone()).ok();
            // Waiting changes are retried every half minute, also without the window open.
            let retry = engine.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    if retry.pending().await > 0 {
                        retry.replay().await;
                    }
                }
            });
            // Saved settings and the token from the keyring.
            if let Ok(p) = config_path(&handle)
                && let Ok(bytes) = std::fs::read(p)
                && let Ok(c) = serde_json::from_slice::<Config>(&bytes)
            {
                let state = app.state::<App>();
                if let Some(server) = c.server.clone().filter(|_| c.token_id.is_some()) {
                    let token = tauri::async_runtime::block_on(keyring(server, |e| {
                        match e.get_password() {
                            Ok(t) => Ok(Some(t)),
                            Err(keyring::Error::NoEntry) => Ok(None),
                            Err(e) => Err(e),
                        }
                    }));
                    *state.token.lock().unwrap() = token.ok().flatten();
                }
                *state.config.lock().unwrap() = c;
                engine.set_credentials(credentials(&state));
            }

            let open = MenuItem::with_id(app, "open", "Open Streamline", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().cloned().expect("app icon"))
                .tooltip("Streamline")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id().as_ref() {
                    "open" => show(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, e| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = e
                    {
                        show(tray.app_handle());
                    }
                })
                .build(app)?;
            if !minimized {
                show(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps the app (and syncing) in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Streamline");
}
