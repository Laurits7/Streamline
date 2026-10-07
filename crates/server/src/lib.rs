//! Streamline server: HTTP API, auth, realtime events, background jobs and the
//! embedded web app, in one binary.

pub mod auth;
pub mod backup;
pub mod blocks;
pub mod caldav;
pub mod calsync;
pub mod config;
pub mod db;
pub mod deps;
pub mod error;
pub mod events;
pub mod jobs;
pub mod models;
pub mod net;
pub mod notify;
pub mod occasions;
pub mod ownership;
pub mod push;
pub mod reminders;
pub mod rollover;
pub mod routes;
pub mod routines;
pub mod secrets;
pub mod static_files;
pub mod tracking;
pub mod util;
pub mod visibility;
pub mod workflows;

use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Instant};

use tokio::{net::TcpListener, sync::Mutex};
use tracing_subscriber::EnvFilter;

pub struct App {
    pub config: config::Config,
    pub db: db::Db,
    pub bus: events::Bus,
    /// Failed login attempts per key (username or IP): (count, window start).
    pub login_failures: Mutex<HashMap<String, (u32, Instant)>>,
    /// Encrypts stored credentials (CalDAV passwords).
    pub secrets: secrets::Secrets,
    /// Calendar syncs run one at a time.
    pub calendar_lock: Mutex<()>,
    /// Web Push signing key (data/vapid.key).
    pub vapid: push::Vapid,
}

pub type AppState = Arc<App>;

/// Open the database, run migrations and build the router.
pub async fn build(config: config::Config) -> anyhow::Result<(axum::Router, AppState)> {
    std::fs::create_dir_all(&config.data_dir)?;
    let db = db::Db::open(&config.data_dir.join("streamline.db")).await?;
    let secrets = secrets::Secrets::load(&config.data_dir, config.secret_key.as_deref())?;
    let vapid = push::Vapid::load(&config.data_dir, &config.push_contact)?;
    let state = Arc::new(App {
        config,
        db,
        bus: events::Bus::new(),
        login_failures: Mutex::new(HashMap::new()),
        secrets,
        calendar_lock: Mutex::new(()),
        vapid,
    });
    auth::bootstrap_admin(&state).await?;
    Ok((routes::router(state.clone()), state))
}

pub async fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .with_env_filter(
            EnvFilter::try_from_env("LOG_LEVEL")
                .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn")),
        )
        .init();
    let config = config::Config::from_env()?;
    let addr = SocketAddr::new(config.bind, config.port);
    let (app, state) = build(config).await?;
    jobs::spawn(state.clone());
    calsync::spawn(state.clone());
    backup::spawn(state);
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Streamline listening on http://{addr}");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
    tracing::info!("shutting down");
}
