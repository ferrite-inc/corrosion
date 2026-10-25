use std::{net::SocketAddr, path::Path};

use anyhow::{Context, Result};
use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::Deserialize;
use serde_with::{As, DisplayFromStr};
use tracing::level_filters::LevelFilter;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    pub(crate) server: ServerConfig,
    pub(crate) log: LogConfig,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ServerConfig {
    pub(crate) address: SocketAddr,
    pub(crate) token: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LogConfig {
    #[serde(with = "As::<DisplayFromStr>")]
    pub(crate) level: LevelFilter,
    pub(crate) external: ExternalLogConfig,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExternalLogConfig {
    #[serde(with = "As::<DisplayFromStr>")]
    pub(crate) level: LevelFilter,
}

pub(crate) fn load() -> Result<Config> {
    let mut figment = Figment::from(Toml::string(include_str!("config/defaults.toml")));
    if Path::new("corrosion.toml").try_exists()? {
        figment = figment.merge(Toml::file_exact("corrosion.toml"));
    }

    figment
        .merge(Env::prefixed("CORROSION_").split("__"))
        .extract()
        .context("invalid configuration")
}
