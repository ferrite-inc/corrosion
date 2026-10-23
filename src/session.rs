use std::num::NonZeroU64;

use serde::Serialize;
use serde_with::{As, DisplayFromStr};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::player::EndReason;

pub(crate) struct Session {
    pub(crate) application_id: NonZeroU64,
    pub(crate) events: broadcast::Sender<Event>,
    pub(crate) closed: CancellationToken,
}

impl Session {
    pub(crate) fn new(application_id: NonZeroU64) -> Self {
        let (events, _) = broadcast::channel(128);
        Self {
            application_id,
            events,
            closed: CancellationToken::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum Event {
    PlayerChanged {
        #[serde(with = "As::<DisplayFromStr>")]
        guild_id: NonZeroU64,
    },
    PlayerDestroyed {
        #[serde(with = "As::<DisplayFromStr>")]
        guild_id: NonZeroU64,
    },
    TrackStarted {
        #[serde(with = "As::<DisplayFromStr>")]
        guild_id: NonZeroU64,
        item_id: Uuid,
    },
    TrackEnded {
        #[serde(with = "As::<DisplayFromStr>")]
        guild_id: NonZeroU64,
        item_id: Uuid,
        reason: EndReason,
        error: Option<String>,
    },
}
