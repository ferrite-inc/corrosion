use async_trait::async_trait;
use reqwest::Client;
use songbird::{
    Driver, Event, EventContext, EventHandler, TrackEvent,
    events::EventData,
    input::HttpRequest,
    tracks::{PlayMode, Track, TrackHandle},
};
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use super::Notification;
use crate::resolver::StreamUrl;

pub(super) fn play(
    driver: &mut Driver,
    http: &Client,
    url: StreamUrl,
    item_id: Uuid,
    paused: bool,
    notifications: &UnboundedSender<Notification>,
) -> TrackHandle {
    let input = HttpRequest::new(http.clone(), url.into_string());
    let mut track = Track::new_with_uuid(input.into(), item_id);
    if paused {
        track = track.pause();
    }
    for event in [TrackEvent::Playable, TrackEvent::End] {
        track.events.add_event(
            EventData::new(
                event.into(),
                TrackListener {
                    event,
                    notifications: notifications.clone(),
                },
            ),
            std::time::Duration::ZERO,
        );
    }
    driver.play_only(track)
}

struct TrackListener {
    event: TrackEvent,
    notifications: UnboundedSender<Notification>,
}

#[async_trait]
impl EventHandler for TrackListener {
    async fn act(&self, context: &EventContext<'_>) -> Option<Event> {
        let EventContext::Track(tracks) = context else {
            return None;
        };
        let (state, handle) = tracks.first()?;
        let item_id = handle.uuid();
        let notification = match self.event {
            TrackEvent::Playable => Notification::Ready { item_id },
            TrackEvent::End => Notification::Ended {
                item_id,
                failed: matches!(state.playing, PlayMode::Errored(_)),
            },
            _ => return None,
        };
        let _ = self.notifications.send(notification);
        None
    }
}
