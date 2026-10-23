use std::{net::SocketAddr, path::Path};

use anyhow::Context;
use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::Deserialize;

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
    pub(crate) level: String,
}

pub(crate) fn load() -> anyhow::Result<Config> {
    let mut figment = Figment::from(Toml::string(include_str!("config/defaults.toml")));
    if Path::new("corrosion.toml").try_exists()? {
        figment = figment.merge(Toml::file_exact("corrosion.toml"));
    }

    figment
        .merge(Env::prefixed("CORROSION_").split("__"))
        .extract()
        .context("invalid configuration")
}
