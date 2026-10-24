use axum::{
    Json,
    extract::rejection::{JsonRejection, PathRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

use crate::{player, runtime};

pub(super) struct Error {
    status: StatusCode,
    code: &'static str,
}

impl Error {
    pub(super) fn new(status: StatusCode, code: &'static str) -> Self {
        Self { status, code }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.code }))).into_response()
    }
}

impl From<runtime::Error> for Error {
    fn from(error: runtime::Error) -> Self {
        let (status, code) = match error {
            runtime::Error::SessionNotFound => (StatusCode::NOT_FOUND, "session_not_found"),
            runtime::Error::PlayerNotFound => (StatusCode::NOT_FOUND, "player_not_found"),
            runtime::Error::OwnershipConflict => (StatusCode::CONFLICT, "ownership_conflict"),
            runtime::Error::Closing => (StatusCode::CONFLICT, "closing"),
            runtime::Error::ShuttingDown => (StatusCode::SERVICE_UNAVAILABLE, "shutting_down"),
        };
        Self::new(status, code)
    }
}

impl From<player::Error> for Error {
    fn from(error: player::Error) -> Self {
        let (status, code) = match error {
            player::Error::Busy => (StatusCode::SERVICE_UNAVAILABLE, "player_busy"),
            player::Error::Closed => (StatusCode::GONE, "player_closed"),
            player::Error::QueueFull => (StatusCode::CONFLICT, "queue_full"),
            player::Error::InvalidInput => (StatusCode::BAD_REQUEST, "invalid_input"),
            player::Error::CurrentItemMismatch => (StatusCode::CONFLICT, "current_item_mismatch"),
            player::Error::InvalidVoice => (StatusCode::BAD_REQUEST, "invalid_voice"),
        };
        Self::new(status, code)
    }
}

impl From<JsonRejection> for Error {
    fn from(error: JsonRejection) -> Self {
        Self::new(error.status(), "invalid_request")
    }
}

impl From<PathRejection> for Error {
    fn from(_: PathRejection) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_path")
    }
}
