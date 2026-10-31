use axum::{
    extract::State,
    response::sse::{Event as SseEvent, KeepAlive, Sse},
};
use futures_util::{Stream, StreamExt, stream};
use uuid::Uuid;

use crate::{
    api::{error::Error, extract::Path},
    runtime::Runtime,
    session::Event,
};

pub(super) async fn subscribe(
    State(runtime): State<Runtime>,
    Path(session_id): Path<Uuid>,
) -> Result<Sse<impl Stream<Item = Result<SseEvent, axum::Error>>>, Error> {
    let session = runtime.session(session_id)?;
    let events = stream::unfold(session.events.subscribe(), |mut receiver| async move {
        // End the stream if events were lost or the channel closed.
        let event = receiver.recv().await.ok()?;
        Some((encode(&event), receiver))
    })
    .take_until(session.closed.clone().cancelled_owned());

    Ok(Sse::new(events).keep_alive(KeepAlive::default()))
}

fn encode(event: &Event) -> Result<SseEvent, axum::Error> {
    let name = match event {
        Event::PlayerChanged { .. } => "player_changed",
        Event::PlayerDestroyed { .. } => "player_destroyed",
        Event::TrackStarted { .. } => "track_started",
        Event::TrackEnded { .. } => "track_ended",
    };

    SseEvent::default().event(name).json_data(event)
}
