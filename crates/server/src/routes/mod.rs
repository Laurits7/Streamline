mod account;
mod days;
mod projects;
mod sync;
mod tasks;

use axum::{
    Router,
    routing::{get, patch, post},
};
use tower_http::{compression::CompressionLayer, trace::TraceLayer};

use crate::AppState;

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/setup", get(account::setup_status).post(account::setup))
        .route("/auth/login", post(account::login))
        .route("/auth/logout", post(account::logout))
        .route("/me", get(account::me).patch(account::patch_me))
        .route("/me/password", post(account::change_password))
        .route(
            "/tokens",
            get(account::list_tokens).post(account::create_token),
        )
        .route("/tokens/{id}", axum::routing::delete(account::revoke_token))
        .route(
            "/users",
            get(account::list_users).post(account::create_user),
        )
        .route(
            "/users/{id}",
            patch(account::patch_user).delete(account::delete_user),
        )
        .route("/sync", get(sync::sync))
        .route("/events", get(sync::events))
        .route("/today", get(sync::today))
        .route("/task-types", get(sync::task_types))
        .route("/projects", get(projects::list).post(projects::create))
        .route(
            "/projects/{id}",
            patch(projects::patch).delete(projects::delete),
        )
        .route("/tasks", get(tasks::list).post(tasks::create))
        .route(
            "/tasks/{id}",
            get(tasks::get_one)
                .patch(tasks::patch)
                .delete(tasks::delete),
        )
        .route("/days/{date}", get(days::get_day))
        .route("/days/{date}/entries", post(days::add_entry))
        .route(
            "/day-entries/{id}",
            patch(days::patch_entry).delete(days::delete_entry),
        );

    Router::new()
        .nest("/api/v1", api)
        .route("/healthz", get(|| async { "ok" }))
        .fallback(crate::static_files::serve)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
