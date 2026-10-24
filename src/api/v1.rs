mod events;

use std::num::NonZeroU64;

use axum::{
    Router,
    extract::State,
    http::StatusCode,
    routing::{delete, get, post, put},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    api::{
        error::Error,
        extract::{Json, Path},
    },
    player::{QueueItem, Snapshot, VoiceParameters},
    resolver::Resolver,
    runtime::Runtime,
};

pub(super) fn router() -> Router<Runtime> {
    Router::new()
        .route("/sessions", post(create_session))
        .route("/sessions/{session}", delete(destroy_session))
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
    Json(request): Json<CreateSession>,
) -> Result<(StatusCode, Json<Session>), Error> {
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
    Path(session): Path<Uuid>,
) -> Result<StatusCode, Error> {
    runtime.destroy_session(session).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_players(
    State(runtime): State<Runtime>,
    Path(session): Path<Uuid>,
) -> Result<Json<Vec<Snapshot>>, Error> {
    let players = runtime.players(session)?;
    let mut snapshots = Vec::with_capacity(players.len());
    for player in players {
        snapshots.push(player.snapshot().await?);
    }
    Ok(Json(snapshots))
}

async fn get_player(
    State(runtime): State<Runtime>,
    Path((session, guild)): Path<(Uuid, String)>,
) -> Result<Json<Snapshot>, Error> {
    let player = runtime.player(session, parse_id(&guild)?)?;
    Ok(Json(player.snapshot().await?))
}

async fn update_voice(
    State(runtime): State<Runtime>,
    Path((session, guild)): Path<(Uuid, String)>,
    Json(request): Json<UpdateVoice>,
) -> Result<StatusCode, Error> {
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
    Path((session, guild)): Path<(Uuid, String)>,
    Json(request): Json<Enqueue>,
) -> Result<(StatusCode, Json<QueueItem>), Error> {
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
    Path((session, guild)): Path<(Uuid, String)>,
    Json(request): Json<SetPaused>,
) -> Result<StatusCode, Error> {
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
    Path((session, guild)): Path<(Uuid, String)>,
    Json(request): Json<Skip>,
) -> Result<StatusCode, Error> {
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
    Path((session, guild)): Path<(Uuid, String)>,
) -> Result<StatusCode, Error> {
    runtime.destroy_player(session, parse_id(&guild)?).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_id(raw: &str) -> Result<NonZeroU64, Error> {
    raw.parse()
        .map_err(|_| Error::new(StatusCode::BAD_REQUEST, "invalid_id"))
}
