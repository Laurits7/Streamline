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

/// The OpenAPI 3.1 description of `/api/v1`, generated from the handlers' annotations.
#[derive(utoipa::OpenApi)]
#[openapi(
    info(
        title = "Streamline API",
        description = "Self-hosted household todo and day planner. Authenticate with the session cookie or `Authorization: Bearer <token>` (create tokens in Settings). Ids are ULIDs; clients may supply their own when creating records. Every record has `rev`; `GET /sync?since=<rev>` returns what changed, including deletions (`deleted_at`).",
    ),
    servers((url = "/api/v1")),
    modifiers(&Security),
    security(("bearer" = []), ("cookie" = [])),
    paths(
        account::setup_status, account::setup, account::login, account::logout, account::me,
        account::patch_me, account::change_password, account::list_tokens, account::create_token,
        account::revoke_token, account::list_users, account::create_user, account::patch_user,
        account::delete_user,
        sync::sync, sync::events, sync::today, sync::task_types,
        projects::list, projects::create, projects::patch, projects::delete,
        tasks::list, tasks::create, tasks::get_one, tasks::patch, tasks::delete,
        days::get_day, days::add_entry, days::put_plan, days::delete_plan, days::patch_entry, days::delete_entry,
    ),
    tags(
        (name = "account", description = "Setup, sign-in, profile and API tokens"),
        (name = "users", description = "User management (admins)"),
        (name = "sync", description = "Change feed and live events"),
        (name = "projects", description = "Projects and subprojects"),
        (name = "tasks", description = "Tasks"),
        (name = "days", description = "The day plan"),
    )
)]
pub struct ApiDoc;

struct Security;

impl utoipa::Modify for Security {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{
            ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme,
        };
        let c = openapi.components.get_or_insert_with(Default::default);
        c.add_security_scheme(
            "bearer",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
        c.add_security_scheme(
            "cookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new(
                crate::auth::SESSION_COOKIE,
            ))),
        );
    }
}

async fn openapi_json() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(<ApiDoc as utoipa::OpenApi>::openapi())
}

/// A small page rendering the OpenAPI document (the viewer script loads from a CDN).
async fn api_docs() -> axum::response::Html<&'static str> {
    axum::response::Html(
        r#"<!doctype html><html><head><meta charset="utf-8"><title>Streamline API</title>
<meta name="viewport" content="width=device-width, initial-scale=1"></head><body>
<script id="api-reference" data-url="/api/v1/openapi.json"></script>
<script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
</body></html>"#,
    )
}

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
            "/days/{date}/plan",
            axum::routing::put(days::put_plan).delete(days::delete_plan),
        )
        .route(
            "/day-entries/{id}",
            patch(days::patch_entry).delete(days::delete_entry),
        )
        .route("/openapi.json", get(openapi_json));

    Router::new()
        .nest("/api/v1", api)
        .route("/api/docs", get(api_docs))
        .route("/healthz", get(|| async { "ok" }))
        .fallback(crate::static_files::serve)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
