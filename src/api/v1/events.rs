use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::{Stream, StreamExt as _};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};
use uuid::Uuid;

use crate::{
    api::{error::Error, extract::Path},
    runtime::Runtime,
    session,
};

pub(super) async fn subscribe(
    State(runtime): State<Runtime>,
    Path(id): Path<Uuid>,
) -> Result<Sse<impl Stream<Item = Result<Event, axum::Error>>>, Error> {
    let session = runtime.session(id)?;
    let stream = BroadcastStream::new(session.events.subscribe())
        .map_while(|event| event.ok().map(encode))
        .take_until(session.closed.clone().cancelled_owned());
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

fn encode(event: session::Event) -> Result<Event, axum::Error> {
    let name = match &event {
        session::Event::PlayerChanged { .. } => "player_changed",
        session::Event::PlayerDestroyed { .. } => "player_destroyed",
        session::Event::TrackStarted { .. } => "track_started",
        session::Event::TrackEnded { .. } => "track_ended",
    };
    Event::default().event(name).json_data(event)
}
