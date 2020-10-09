use crate::error::Result;
use anyhow::Context;
use log::*;
use serde::Deserialize;
use std::{env, env::VarError, fs::File, io::prelude::*};

#[derive(Deserialize, Debug)]
pub struct Config {
    pub general: GeneralConfig,
    pub auth: AuthConfig,
}

#[derive(Deserialize, Debug)]
pub struct GeneralConfig {
    pub command_prefix: String,
}

#[derive(Deserialize, Debug)]
pub struct AuthConfig {
    pub token: String,
    pub superuser: u64,
}

pub fn read() -> Result<Config> {
    let path = match env::var("BOT_GEORGE_CONFIG") {
        Ok(s) => s,
        Err(VarError::NotPresent) => "config.toml".into(),
        Err(e) => return Err(e).context("couldn't read env var BOT_GEORGE_CONFIG"),
    };

    let mut vec = vec![];

    File::open(&path)
        .context("failed to open config file")?
        .read_to_end(&mut vec)
        .context("failed to read config file")?;

    let config = toml::from_slice(&vec).context("failed to parse TOML")?;

    info!("Config loaded from {:?}", path);

    Ok(config)
}
