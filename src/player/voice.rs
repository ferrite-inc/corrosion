use std::num::NonZeroU64;

use async_trait::async_trait;
use songbird::{ConnectionInfo, CoreEvent, Driver, Event, EventContext, EventHandler};
use tokio::sync::mpsc::UnboundedSender;
use url::Url;
use uuid::Uuid;

use super::{Error, Notification};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct VoiceParameters {
    channel_id: NonZeroU64,
    endpoint: String,
    session_id: String,
    token: String,
}

impl VoiceParameters {
    pub(crate) fn new(
        channel_id: NonZeroU64,
        endpoint: String,
        session_id: String,
        token: String,
    ) -> Result<Self, Error> {
        if session_id.is_empty() || token.is_empty() {
            return Err(Error::InvalidVoice);
        }

        // Reject URL delimiters and characters the parser may strip.
        if endpoint.contains(['/', '\\', '?', '#', '@'])
            || endpoint.contains(char::is_whitespace)
            || endpoint.contains(char::is_control)
        {
            return Err(Error::InvalidVoice);
        }

        Url::parse(&format!("wss://{endpoint}")).map_err(|_| Error::InvalidVoice)?;

        Ok(Self {
            channel_id,
            endpoint,
            session_id,
            token,
        })
    }
}

pub(super) fn connect(
    driver: &mut Driver,
    application_id: NonZeroU64,
    guild_id: NonZeroU64,
    parameters: VoiceParameters,
    attempt: Uuid,
    notifications: &UnboundedSender<Notification>,
) -> impl Future<Output = Notification> + Send + 'static {
    driver.remove_all_global_events();
    for event in [CoreEvent::DriverReconnect, CoreEvent::DriverDisconnect] {
        driver.add_global_event(
            event.into(),
            VoiceListener {
                attempt,
                channel_id: parameters.channel_id,
                session_id: parameters.session_id.clone(),
                notifications: notifications.clone(),
            },
        );
    }
    let connected = driver.connect(ConnectionInfo {
        channel_id: parameters.channel_id.into(),
        guild_id: guild_id.into(),
        user_id: application_id.into(),
        endpoint: parameters.endpoint,
        session_id: parameters.session_id,
        token: parameters.token,
    });
    async move {
        let connected = connected.await.is_ok();
        Notification::Voice { attempt, connected }
    }
}

struct VoiceListener {
    attempt: Uuid,
    channel_id: NonZeroU64,
    session_id: String,
    notifications: UnboundedSender<Notification>,
}

#[async_trait]
impl EventHandler for VoiceListener {
    async fn act(&self, context: &EventContext<'_>) -> Option<Event> {
        let connected = match context {
            EventContext::DriverReconnect(connection)
                if connection.channel_id.0 == self.channel_id
                    && connection.session_id == self.session_id =>
            {
                true
            }
            EventContext::DriverDisconnect(connection)
                if connection.channel_id.0 == self.channel_id
                    && connection.session_id == self.session_id
                    && connection.reason.is_some() =>
            {
                false
            }
            _ => return None,
        };
        let _ = self.notifications.send(Notification::Voice {
            attempt: self.attempt,
            connected,
        });
        None
    }
}
