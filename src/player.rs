mod track;
mod voice;

use std::{collections::VecDeque, num::NonZeroU64};

use reqwest::Client;
use serde::Serialize;
use serde_with::{As, DisplayFromStr};
use songbird::{Config, Driver, tracks::TrackHandle};
use thiserror::Error;
use tokio::{
    sync::{
        broadcast,
        mpsc::{self, error::TrySendError},
        oneshot,
    },
    task::{AbortHandle, JoinSet},
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::error;
use uuid::Uuid;
pub(crate) use voice::VoiceParameters;

use crate::{
    resolver::{Resolver, StreamUrl},
    session::Event,
};

const COMMAND_CAPACITY: usize = 64;
const MAX_QUEUE_ITEMS: usize = 1000;
const MAX_INPUT_BYTES: usize = 8192;

#[derive(Clone)]
pub(crate) struct PlayerHandle {
    commands: mpsc::Sender<Command>,
    cancellation: CancellationToken,
    finished: TaskTracker,
}

impl PlayerHandle {
    pub(crate) fn spawn(
        application_id: NonZeroU64,
        guild_id: NonZeroU64,
        http: Client,
        config: Config,
        events: broadcast::Sender<Event>,
    ) -> Self {
        let (command_sender, commands) = mpsc::channel(COMMAND_CAPACITY);
        let (notification_sender, notifications) = mpsc::unbounded_channel();
        let finished = TaskTracker::new();
        let cancellation = CancellationToken::new();
        let player = Player {
            application_id,
            guild_id,
            driver: Driver::new(config),
            http,
            events,
            commands,
            notification_sender,
            notifications,
            cancellation: cancellation.clone(),
            workers: JoinSet::new(),
            current: None,
            queue: VecDeque::new(),
            voice: VoiceState::Disconnected,
            voice_parameters: None,
            voice_attempt: Uuid::nil(),
            voice_task: None,
            last_error: None,
        };
        finished.spawn(player.run());
        finished.close();
        Self {
            commands: command_sender,
            cancellation,
            finished,
        }
    }

    pub(crate) async fn snapshot(&self) -> Result<Snapshot, Error> {
        self.request(Command::Snapshot).await
    }

    pub(crate) async fn enqueue(
        &self,
        resolver: Resolver,
        input: String,
    ) -> Result<QueueItem, Error> {
        self.request(|reply| Command::Enqueue {
            resolver,
            input,
            reply,
        })
        .await
    }

    pub(crate) async fn set_paused(&self, paused: bool) -> Result<(), Error> {
        self.request(|reply| Command::SetPaused { paused, reply })
            .await
    }

    pub(crate) async fn skip(&self, expected_current_item_id: Uuid) -> Result<(), Error> {
        self.request(|reply| Command::Skip {
            expected_current_item_id,
            reply,
        })
        .await
    }

    pub(crate) async fn update_voice(&self, parameters: VoiceParameters) -> Result<(), Error> {
        self.request(|reply| Command::UpdateVoice { parameters, reply })
            .await
    }

    pub(crate) fn close(&self) {
        self.cancellation.cancel();
    }

    pub(crate) fn is_closing(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    pub(crate) async fn closed(self) {
        self.finished.wait().await;
    }

    async fn request<T>(&self, command: impl FnOnce(Reply<T>) -> Command) -> Result<T, Error> {
        if self.is_closing() {
            return Err(Error::Closed);
        }
        let (reply, response) = oneshot::channel();
        self.commands
            .try_send(command(reply))
            .map_err(|error| match error {
                TrySendError::Full(_) => Error::Busy,
                TrySendError::Closed(_) => Error::Closed,
            })?;
        response.await.map_err(|_| Error::Closed)?
    }
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
pub(crate) struct QueueItem {
    pub(crate) id: Uuid,
    pub(crate) resolver: Resolver,
    pub(crate) input: String,
}

#[derive(Serialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(rename = "PlayerSnapshot"))]
pub(crate) struct Snapshot {
    #[serde(with = "As::<DisplayFromStr>")]
    #[cfg_attr(feature = "bindings", ts(type = "string"))]
    pub(crate) guild_id: NonZeroU64,
    pub(crate) voice: VoiceState,
    pub(crate) playback: PlaybackState,
    pub(crate) current: Option<QueueItem>,
    pub(crate) queue: Vec<QueueItem>,
    pub(crate) last_error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub(crate) enum VoiceState {
    Disconnected,
    Connecting,
    Connected,
    Failed,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub(crate) enum PlaybackState {
    Idle,
    Resolving,
    Loading,
    Playing,
    Paused,
    WaitingForVoice,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub(crate) enum EndReason {
    Finished,
    Skipped,
    Failed,
}

#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error("player command queue is full")]
    Busy,
    #[error("player is closed")]
    Closed,
    #[error("queue item limit reached")]
    QueueFull,
    #[error("input must contain between 1 and 8192 bytes")]
    InvalidInput,
    #[error("current item changed")]
    CurrentItemMismatch,
    #[error("invalid voice parameters")]
    InvalidVoice,
}

type Reply<T> = oneshot::Sender<Result<T, Error>>;

enum Command {
    Snapshot(Reply<Snapshot>),
    Enqueue {
        resolver: Resolver,
        input: String,
        reply: Reply<QueueItem>,
    },
    SetPaused {
        paused: bool,
        reply: Reply<()>,
    },
    Skip {
        expected_current_item_id: Uuid,
        reply: Reply<()>,
    },
    UpdateVoice {
        parameters: VoiceParameters,
        reply: Reply<()>,
    },
}

enum Notification {
    Resolved {
        item_id: Uuid,
        result: Result<StreamUrl, String>,
    },
    Ready {
        item_id: Uuid,
    },
    Ended {
        item_id: Uuid,
        failed: bool,
    },
    Voice {
        attempt: Uuid,
        connected: bool,
    },
}

struct Player {
    application_id: NonZeroU64,
    guild_id: NonZeroU64,
    driver: Driver,
    http: Client,
    events: broadcast::Sender<Event>,
    commands: mpsc::Receiver<Command>,
    notification_sender: mpsc::UnboundedSender<Notification>,
    notifications: mpsc::UnboundedReceiver<Notification>,
    cancellation: CancellationToken,
    workers: JoinSet<Notification>,
    current: Option<Current>,
    queue: VecDeque<QueueItem>,
    voice: VoiceState,
    voice_parameters: Option<VoiceParameters>,
    voice_attempt: Uuid,
    voice_task: Option<AbortHandle>,
    last_error: Option<String>,
}

impl Player {
    async fn run(mut self) {
        loop {
            let changed = tokio::select! {
                () = self.cancellation.cancelled() => break,
                command = self.commands.recv() => {
                    let Some(command) = command else { break };
                    self.apply(command)
                }
                Some(notification) = self.notifications.recv() => self.notify(notification),
                Some(result) = self.workers.join_next() => {
                    match result {
                        Ok(notification) => self.notify(notification),
                        Err(error) => {
                            if !error.is_cancelled() {
                                error!(%error, "player worker failed");
                            }
                            false
                        }
                    }
                }
            };
            let advanced = self.maintain();
            if changed || advanced {
                self.publish(Event::PlayerChanged {
                    guild_id: self.guild_id,
                });
            }
        }

        self.commands.close();
        self.notifications.close();
        self.stop_current();
        self.queue.clear();
        self.workers.abort_all();
        while self.workers.join_next().await.is_some() {}
        self.driver.stop();
        self.driver.leave();
    }

    fn apply(&mut self, command: Command) -> bool {
        match command {
            Command::Snapshot(reply) => {
                let _ = reply.send(Ok(self.snapshot()));
                false
            }
            Command::Enqueue {
                resolver,
                input,
                reply,
            } => {
                if input.trim().is_empty() || input.len() > MAX_INPUT_BYTES {
                    let _ = reply.send(Err(Error::InvalidInput));
                    return false;
                }
                if self.queue.len() + usize::from(self.current.is_some()) >= MAX_QUEUE_ITEMS {
                    let _ = reply.send(Err(Error::QueueFull));
                    return false;
                }
                let item = QueueItem {
                    id: Uuid::new_v4(),
                    resolver,
                    input,
                };
                self.queue.push_back(item.clone());
                let _ = reply.send(Ok(item));
                true
            }
            Command::SetPaused { paused, reply } => {
                let changed = self.current.as_mut().is_some_and(|current| {
                    let changed = current.paused != paused;
                    current.paused = paused;
                    changed
                });
                let _ = reply.send(Ok(()));
                changed
            }
            Command::Skip {
                expected_current_item_id,
                reply,
            } => {
                if self.current.as_ref().map(|current| current.item.id)
                    != Some(expected_current_item_id)
                {
                    let _ = reply.send(Err(Error::CurrentItemMismatch));
                    return false;
                }
                self.finish_current(EndReason::Skipped, None);
                let _ = reply.send(Ok(()));
                true
            }
            Command::UpdateVoice { parameters, reply } => {
                if self.voice_parameters.as_ref() == Some(&parameters)
                    && self.voice != VoiceState::Failed
                {
                    let _ = reply.send(Ok(()));
                    return false;
                }
                if let Some(task) = self.voice_task.take() {
                    task.abort();
                }
                self.voice = VoiceState::Connecting;
                self.voice_attempt = Uuid::new_v4();
                self.voice_parameters = Some(parameters.clone());
                self.voice_task = Some(self.workers.spawn(voice::connect(
                    &mut self.driver,
                    self.application_id,
                    self.guild_id,
                    parameters,
                    self.voice_attempt,
                    &self.notification_sender,
                )));
                let _ = reply.send(Ok(()));
                true
            }
        }
    }

    fn notify(&mut self, notification: Notification) -> bool {
        match notification {
            Notification::Resolved { item_id, result } => {
                let Some(current) = &mut self.current else {
                    return false;
                };
                if current.item.id != item_id || !matches!(current.stage, Stage::Resolving { .. }) {
                    return false;
                }
                match result {
                    Ok(url) if self.voice == VoiceState::Connected => {
                        let handle = track::play(
                            &mut self.driver,
                            &self.http,
                            url,
                            item_id,
                            current.paused,
                            &self.notification_sender,
                        );
                        current.stage = Stage::Active {
                            handle,
                            ready: false,
                            started: false,
                            suspended: current.paused,
                        };
                    }
                    Ok(_) => current.stage = Stage::Waiting,
                    Err(error) => self.finish_current(EndReason::Failed, Some(error)),
                }
                true
            }
            Notification::Ready { item_id } => {
                let Some(current) = &mut self.current else {
                    return false;
                };
                if current.item.id != item_id {
                    return false;
                }
                let Stage::Active { ready, .. } = &mut current.stage else {
                    return false;
                };
                *ready = true;
                true
            }
            Notification::Ended { item_id, failed } => {
                if self.current.as_ref().map(|current| current.item.id) != Some(item_id) {
                    return false;
                }
                let (reason, error) = if failed {
                    (EndReason::Failed, Some("audio playback failed".to_owned()))
                } else {
                    (EndReason::Finished, None)
                };
                self.finish_current(reason, error);
                true
            }
            Notification::Voice { attempt, connected } => {
                if attempt != self.voice_attempt || self.voice == VoiceState::Failed {
                    return false;
                }
                self.voice = if connected {
                    VoiceState::Connected
                } else {
                    VoiceState::Failed
                };
                if !connected {
                    self.last_error = Some("voice connection failed".to_owned());
                    self.driver.leave();
                }
                true
            }
        }
    }

    fn maintain(&mut self) -> bool {
        let mut changed = false;
        if self.current.is_none() {
            self.current = self.queue.pop_front().map(|item| Current {
                item,
                paused: false,
                stage: Stage::Waiting,
            });
            changed = self.current.is_some();
        }
        let Some(current) = &mut self.current else {
            return changed;
        };
        if self.voice == VoiceState::Connected && matches!(current.stage, Stage::Waiting) {
            let item_id = current.item.id;
            let resolver = current.item.resolver;
            let input = current.item.input.clone();
            let task = self.workers.spawn(async move {
                let result = resolver.resolve(&input).map_err(|error| error.to_string());
                Notification::Resolved { item_id, result }
            });
            current.stage = Stage::Resolving { task };
            changed = true;
        }
        if let Stage::Active {
            handle,
            ready,
            started,
            suspended,
        } = &mut current.stage
        {
            let should_suspend = current.paused || self.voice != VoiceState::Connected;
            if *suspended != should_suspend {
                if should_suspend {
                    let _ = handle.pause();
                } else {
                    let _ = handle.play();
                }
                *suspended = should_suspend;
            }
            if *ready && !*started && !should_suspend {
                *started = true;
                let event = Event::TrackStarted {
                    guild_id: self.guild_id,
                    item_id: current.item.id,
                };
                self.publish(event);
                changed = true;
            }
        }
        changed
    }

    fn snapshot(&self) -> Snapshot {
        let playback = match &self.current {
            None => PlaybackState::Idle,
            Some(current) if current.paused => PlaybackState::Paused,
            Some(_) if self.voice != VoiceState::Connected => PlaybackState::WaitingForVoice,
            Some(current) => match current.stage {
                Stage::Waiting | Stage::Resolving { .. } => PlaybackState::Resolving,
                Stage::Active { ready: false, .. } => PlaybackState::Loading,
                Stage::Active { ready: true, .. } => PlaybackState::Playing,
            },
        };
        Snapshot {
            guild_id: self.guild_id,
            voice: self.voice,
            playback,
            current: self.current.as_ref().map(|current| current.item.clone()),
            queue: self.queue.iter().cloned().collect(),
            last_error: self.last_error.clone(),
        }
    }

    fn finish_current(&mut self, reason: EndReason, error: Option<String>) {
        let Some(item_id) = self.current.as_ref().map(|current| current.item.id) else {
            return;
        };
        self.stop_current();
        if let Some(error) = &error {
            self.last_error = Some(error.clone());
        }
        self.publish(Event::TrackEnded {
            guild_id: self.guild_id,
            item_id,
            reason,
            error,
        });
    }

    fn stop_current(&mut self) {
        if let Some(current) = self.current.take() {
            match current.stage {
                Stage::Waiting => {}
                Stage::Resolving { task } => task.abort(),
                Stage::Active { handle, .. } => {
                    let _ = handle.stop();
                }
            }
        }
    }

    fn publish(&self, event: Event) {
        let _ = self.events.send(event);
    }
}

struct Current {
    item: QueueItem,
    paused: bool,
    stage: Stage,
}

enum Stage {
    Waiting,
    Resolving {
        task: AbortHandle,
    },
    Active {
        handle: TrackHandle,
        ready: bool,
        started: bool,
        suspended: bool,
    },
}
