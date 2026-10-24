use std::{
    collections::HashMap,
    num::NonZeroU64,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use songbird::driver::DisposalThread;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    player::PlayerHandle,
    session::{Event, Session},
};

#[derive(Clone)]
pub(crate) struct Runtime {
    inner: Arc<Inner>,
}

impl Runtime {
    pub(crate) fn new() -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(15))
            .build()?;
        let config = songbird::Config::default().disposer(DisposalThread::run());
        Ok(Self {
            inner: Arc::new(Inner {
                registry: Mutex::new(Registry::default()),
                http,
                config,
                shutdown_complete: CancellationToken::new(),
            }),
        })
    }

    pub(crate) fn create_session(&self, application_id: NonZeroU64) -> Result<Uuid, Error> {
        let mut registry = self.registry();
        if registry.shutting_down {
            return Err(Error::ShuttingDown);
        }

        let id = Uuid::new_v4();
        registry.sessions.insert(
            id,
            SessionEntry {
                session: Arc::new(Session::new(application_id)),
                players: HashMap::new(),
            },
        );
        Ok(id)
    }

    pub(crate) fn session(&self, id: Uuid) -> Result<Arc<Session>, Error> {
        Ok(self.registry().session(id)?.session.clone())
    }

    pub(crate) fn player(
        &self,
        session_id: Uuid,
        guild_id: NonZeroU64,
    ) -> Result<PlayerHandle, Error> {
        let registry = self.registry();
        let player = registry
            .session(session_id)?
            .players
            .get(&guild_id)
            .ok_or(Error::PlayerNotFound)?;
        if player.is_closing() {
            return Err(Error::Closing);
        }
        Ok(player.clone())
    }

    pub(crate) fn ensure_player(
        &self,
        session_id: Uuid,
        guild_id: NonZeroU64,
    ) -> Result<PlayerHandle, Error> {
        let mut registry = self.registry();
        let entry = registry.session(session_id)?;
        if let Some(player) = entry.players.get(&guild_id) {
            return if player.is_closing() {
                Err(Error::Closing)
            } else {
                Ok(player.clone())
            };
        }

        let session = &entry.session;
        if registry.sessions.values().any(|entry| {
            entry.session.application_id == session.application_id
                && entry.players.contains_key(&guild_id)
        }) {
            return Err(Error::OwnershipConflict);
        }

        let handle = PlayerHandle::spawn(
            session.application_id,
            guild_id,
            self.inner.http.clone(),
            self.inner.config.clone(),
            session.events.clone(),
        );
        registry
            .sessions
            .get_mut(&session_id)
            .expect("session remains registered while locked")
            .players
            .insert(guild_id, handle.clone());
        Ok(handle)
    }

    pub(crate) fn players(&self, session_id: Uuid) -> Result<Vec<PlayerHandle>, Error> {
        Ok(self
            .registry()
            .session(session_id)?
            .players
            .values()
            .filter(|player| !player.is_closing())
            .cloned()
            .collect())
    }

    pub(crate) async fn destroy_player(
        &self,
        session_id: Uuid,
        guild_id: NonZeroU64,
    ) -> Result<(), Error> {
        let handle = {
            let registry = self.registry();
            let player = registry
                .session(session_id)?
                .players
                .get(&guild_id)
                .ok_or(Error::PlayerNotFound)?;
            if player.is_closing() {
                return Err(Error::Closing);
            }
            player.close();
            player.clone()
        };

        let runtime = self.clone();
        tokio::spawn(async move {
            handle.closed().await;
            let mut registry = runtime.registry();
            if let Some(entry) = registry.sessions.get_mut(&session_id)
                && entry.players.remove(&guild_id).is_some()
            {
                let _ = entry
                    .session
                    .events
                    .send(Event::PlayerDestroyed { guild_id });
            }
        })
        .await
        .expect("player cleanup task panicked");
        Ok(())
    }

    pub(crate) async fn destroy_session(&self, session_id: Uuid) -> Result<(), Error> {
        let players = {
            let registry = self.registry();
            let entry = registry.session(session_id)?;
            entry.session.closed.cancel();
            let mut players = Vec::with_capacity(entry.players.len());
            for player in entry.players.values() {
                player.close();
                players.push(player.clone());
            }
            players
        };

        let runtime = self.clone();
        tokio::spawn(async move {
            for player in players {
                player.closed().await;
            }
            runtime.registry().sessions.remove(&session_id);
        })
        .await
        .expect("session cleanup task panicked");
        Ok(())
    }

    pub(crate) async fn shutdown(&self) {
        let players = {
            let mut registry = self.registry();
            if registry.shutting_down {
                None
            } else {
                registry.shutting_down = true;
                let mut players = Vec::new();
                for entry in registry.sessions.values() {
                    entry.session.closed.cancel();
                    for player in entry.players.values() {
                        player.close();
                        players.push(player.clone());
                    }
                }
                Some(players)
            }
        };

        if let Some(players) = players {
            let runtime = self.clone();
            tokio::spawn(async move {
                for player in players {
                    player.closed().await;
                }
                runtime.registry().sessions.clear();
                runtime.inner.shutdown_complete.cancel();
            });
        }
        self.inner.shutdown_complete.cancelled().await;
    }

    fn registry(&self) -> MutexGuard<'_, Registry> {
        self.inner
            .registry
            .lock()
            .expect("runtime registry poisoned")
    }
}

struct Inner {
    registry: Mutex<Registry>,
    http: reqwest::Client,
    config: songbird::Config,
    shutdown_complete: CancellationToken,
}

#[derive(Default)]
struct Registry {
    sessions: HashMap<Uuid, SessionEntry>,
    shutting_down: bool,
}

impl Registry {
    fn session(&self, id: Uuid) -> Result<&SessionEntry, Error> {
        if self.shutting_down {
            return Err(Error::ShuttingDown);
        }
        let entry = self.sessions.get(&id).ok_or(Error::SessionNotFound)?;
        if entry.session.closed.is_cancelled() {
            return Err(Error::Closing);
        }
        Ok(entry)
    }
}

struct SessionEntry {
    session: Arc<Session>,
    players: HashMap<NonZeroU64, PlayerHandle>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("session not found")]
    SessionNotFound,
    #[error("player not found")]
    PlayerNotFound,
    #[error("another session owns this application's player for the guild")]
    OwnershipConflict,
    #[error("resource is closing")]
    Closing,
    #[error("server is shutting down")]
    ShuttingDown,
}
