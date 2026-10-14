//! The server inside the desktop app, for using Streamline on one computer without a
//! separate server (D-88). It listens only on 127.0.0.1 on a port the system picks, runs
//! the usual background jobs (routines, rollover, reminders, calendar sync, backups) while
//! the app runs, and keeps its data folder exactly as a standalone server would, so the
//! folder can later be used to start a real server.

use std::{net::SocketAddr, path::PathBuf};

use tokio::net::TcpListener;

use crate::{AppState, auth, config::Config};

/// The name of the token the app uses; a new one replaces it on every start.
const TOKEN_NAME: &str = "This computer";

pub struct Local {
    /// `http://127.0.0.1:<port>`
    pub url: String,
    /// A fresh API token for the computer's user.
    pub token: String,
    pub state: AppState,
}

/// Start the server on `data_dir` (created if needed). The first start creates the user
/// (`display_name`; a random password, since the app signs itself in).
pub async fn start(data_dir: PathBuf, display_name: &str) -> anyhow::Result<Local> {
    let (app, state) = crate::build(Config::local(data_dir)).await?;
    // The user exists before anything listens: with no users, first-run setup is open,
    // and another account on this computer could claim the server (security review).
    let token = token(&state, display_name).await?;
    crate::jobs::spawn(state.clone());
    crate::calsync::spawn(state.clone());
    crate::backup::spawn(state.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}", listener.local_addr()?);
    tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await;
    });
    Ok(Local { url, token, state })
}

async fn local_user(state: &AppState) -> anyhow::Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM users WHERE is_admin = 1 AND deleted_at IS NULL ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(&state.db.read)
    .await?)
}

/// Give the computer's user a password (the app signs in without one), so the data
/// folder can be used on a real server later. The app owns this server, so no current
/// password is asked.
pub async fn set_password(state: &AppState, password: &str) -> anyhow::Result<()> {
    auth::validate_new_password(password)
        .map_err(|_| anyhow::anyhow!("The password must be at least 8 characters"))?;
    let user = local_user(state)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no user yet"))?;
    let hash = auth::hash_password_async(password.to_string()).await?;
    sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
        .bind(hash)
        .bind(crate::util::now())
        .bind(&user)
        .execute(&state.db.write)
        .await?;
    Ok(())
}

/// The computer's user (the first admin), created on the first start.
async fn token(state: &AppState, display_name: &str) -> anyhow::Result<String> {
    let user = match local_user(state).await? {
        Some(u) => u,
        None => {
            let name = display_name.trim();
            let password = crate::util::random_token();
            auth::create_user(
                state,
                "me",
                &password,
                (!name.is_empty()).then_some(name),
                true,
            )
            .await
            .map_err(|e| anyhow::anyhow!("creating the user: {e:?}"))?
            .id
        }
    };
    sqlx::query("DELETE FROM api_tokens WHERE user_id = ? AND name = ?")
        .bind(&user)
        .bind(TOKEN_NAME)
        .execute(&state.db.write)
        .await?;
    let (token, _) = auth::issue_token(state, &user, TOKEN_NAME)
        .await
        .map_err(|e| anyhow::anyhow!("issuing a token: {e:?}"))?;
    Ok(token)
}
