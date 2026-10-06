use axum::{
    Json,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::json;

/// API error, rendered as RFC 7807 `application/problem+json`.
#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Unauthorized,
    Forbidden(String),
    NotFound,
    Conflict(String),
    TooManyRequests,
    Internal(anyhow::Error),
}

pub type ApiResult<T> = Result<T, AppError>;

pub fn bad(msg: impl Into<String>) -> AppError {
    AppError::BadRequest(msg.into())
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self {
        AppError::Internal(e.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, detail) = match self {
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, Some(m)),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, None),
            AppError::Forbidden(m) => (StatusCode::FORBIDDEN, Some(m)),
            AppError::NotFound => (StatusCode::NOT_FOUND, None),
            AppError::Conflict(m) => (StatusCode::CONFLICT, Some(m)),
            AppError::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                Some("Too many attempts, try again later".into()),
            ),
            AppError::Internal(e) => {
                tracing::error!("internal error: {e:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, None)
            }
        };
        let body = json!({
            "type": "about:blank",
            "title": status.canonical_reason().unwrap_or("Error"),
            "status": status.as_u16(),
            "detail": detail,
        });
        (
            status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            Json(body),
        )
            .into_response()
    }
}
