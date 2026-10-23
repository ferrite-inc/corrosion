mod events;

use std::num::NonZeroU64;

use axum::{
    Json, Router,
    extract::{
        Path, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    player::{self, QueueItem, Snapshot, VoiceParameters},
    resolver::Resolver,
    runtime,
    runtime::Runtime,
};

type SessionPath = Result<Path<Uuid>, PathRejection>;
type PlayerPath = Result<Path<(Uuid, String)>, PathRejection>;

pub(super) fn router() -> Router<Runtime> {
    Router::new()
        .route("/sessions", post(create_session))
        .route(
            "/sessions/{session}",
            axum::routing::delete(destroy_session),
        )
        .route("/sessions/{session}/events", get(events::subscribe))
        .route("/sessions/{session}/players", get(list_players))
        .route(
            "/sessions/{session}/players/{guild}",
            get(get_player).patch(set_paused).delete(destroy_player),
        )
        .route(
            "/sessions/{session}/players/{guild}/voice",
            put(update_voice),
        )
        .route("/sessions/{session}/players/{guild}/queue", post(enqueue))
        .route("/sessions/{session}/players/{guild}/skip", post(skip))
}

async fn create_session(
    State(runtime): State<Runtime>,
    body: Result<Json<CreateSession>, JsonRejection>,
) -> Result<(StatusCode, Json<Session>), Error> {
    let Json(request) = body?;
    let id = runtime.create_session(parse_id(&request.application_id)?)?;
    Ok((StatusCode::CREATED, Json(Session { id })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateSession {
    application_id: String,
}

#[derive(Serialize)]
struct Session {
    id: Uuid,
}

async fn destroy_session(
    State(runtime): State<Runtime>,
    path: SessionPath,
) -> Result<StatusCode, Error> {
    let Path(session) = path?;
    runtime.destroy_session(session).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_players(
    State(runtime): State<Runtime>,
    path: SessionPath,
) -> Result<Json<Vec<Snapshot>>, Error> {
    let Path(session) = path?;
    let players = runtime.players(session)?;
    let mut snapshots = Vec::with_capacity(players.len());
    for player in players {
        snapshots.push(player.snapshot().await?);
    }
    Ok(Json(snapshots))
}

async fn get_player(
    State(runtime): State<Runtime>,
    path: PlayerPath,
) -> Result<Json<Snapshot>, Error> {
    let Path((session, guild)) = path?;
    let player = runtime.player(session, parse_id(&guild)?)?;
    Ok(Json(player.snapshot().await?))
}

async fn update_voice(
    State(runtime): State<Runtime>,
    path: PlayerPath,
    body: Result<Json<UpdateVoice>, JsonRejection>,
) -> Result<StatusCode, Error> {
    let Path((session, guild)) = path?;
    let Json(request) = body?;
    let voice = VoiceParameters::new(
        parse_id(&request.channel_id)?,
        request.endpoint,
        request.session_id,
        request.token,
    )?;
    runtime
        .ensure_player(session, parse_id(&guild)?)?
        .update_voice(voice)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(rename = "VoiceParameters"))]
pub(crate) struct UpdateVoice {
    channel_id: String,
    endpoint: String,
    session_id: String,
    token: String,
}

async fn enqueue(
    State(runtime): State<Runtime>,
    path: PlayerPath,
    body: Result<Json<Enqueue>, JsonRejection>,
) -> Result<(StatusCode, Json<QueueItem>), Error> {
    let Path((session, guild)) = path?;
    let Json(request) = body?;
    let item = runtime
        .player(session, parse_id(&guild)?)?
        .enqueue(request.resolver, request.input)
        .await?;
    Ok((StatusCode::CREATED, Json(item)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(rename = "ResolveInput"))]
pub(crate) struct Enqueue {
    resolver: Resolver,
    input: String,
}

async fn set_paused(
    State(runtime): State<Runtime>,
    path: PlayerPath,
    body: Result<Json<SetPaused>, JsonRejection>,
) -> Result<StatusCode, Error> {
    let Path((session, guild)) = path?;
    let Json(request) = body?;
    runtime
        .player(session, parse_id(&guild)?)?
        .set_paused(request.paused)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetPaused {
    paused: bool,
}

async fn skip(
    State(runtime): State<Runtime>,
    path: PlayerPath,
    body: Result<Json<Skip>, JsonRejection>,
) -> Result<StatusCode, Error> {
    let Path((session, guild)) = path?;
    let Json(request) = body?;
    runtime
        .player(session, parse_id(&guild)?)?
        .skip(request.expected_current_item_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Skip {
    expected_current_item_id: Uuid,
}

async fn destroy_player(
    State(runtime): State<Runtime>,
    path: PlayerPath,
) -> Result<StatusCode, Error> {
    let Path((session, guild)) = path?;
    runtime.destroy_player(session, parse_id(&guild)?).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_id(raw: &str) -> Result<NonZeroU64, Error> {
    raw.parse()
        .map_err(|_| Error::new(StatusCode::BAD_REQUEST, "invalid_id"))
}

struct Error {
    status: StatusCode,
    code: &'static str,
}

impl Error {
    fn new(status: StatusCode, code: &'static str) -> Self {
        Self { status, code }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.status, Json(serde_json::json!({ "error": self.code }))).into_response()
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
