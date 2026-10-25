#![forbid(unsafe_code)]

mod api;
#[cfg(feature = "bindings")]
mod bindings;
mod config;
mod player;
mod resolver;
mod runtime;
mod session;

use std::io;

use anyhow::{Context, Error, Result};
use tokio::net::TcpListener;
use tracing::{error, info};

use crate::{config::LogConfig, runtime::Runtime};

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(feature = "bindings")]
    if std::env::args().nth(1).as_deref() == Some("--export-types") {
        return bindings::export();
    }

    let config = config::load()?;

    init_logging(&config.log)?;

    let runtime = Runtime::new().context("failed to initialize audio resources")?;
    let listener = TcpListener::bind(config.server.address)
        .await
        .context("failed to bind server")?;
    info!(address = %listener.local_addr()?, "listening");

    let app = api::router(runtime.clone(), config.server.token);
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_on_signal(runtime.clone()))
        .await;

    runtime.shutdown().await;
    result.context("server failed")
}

fn init_logging(config: &LogConfig) -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(config.level)
        .try_init()
        .map_err(Error::from_boxed)
}

async fn shutdown_on_signal(runtime: Runtime) {
    if let Err(error) = wait_for_shutdown_signal().await {
        error!(%error, "failed to listen for shutdown signal");
    }
    runtime.shutdown().await;
}

async fn wait_for_shutdown_signal() -> io::Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result,
            _ = terminate.recv() => Ok(()),
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await
}
