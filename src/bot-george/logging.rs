use crate::error::Result;
use anyhow::Context;
use log::*;
use log4rs::{
    append::{console, console::ConsoleAppender},
    config::{Appender, Config, Root},
};
use std::{env, env::VarError, path::PathBuf};

pub fn init() -> Result<()> {
    let (path, opt): (PathBuf, _) = match env::var("BOT_GEORGE_LOGGER_CONFIG") {
        Ok(s) => (s.into(), false),
        Err(VarError::NotPresent) => ("logging.yaml".into(), true),
        Err(e) => return Err(e).context("couldn't read env var BOT_GEORGE_CONFIG"),
    };

    if !opt || path.exists() {
        log4rs::init_file(&path, Default::default())
            .context("failed to set up logging from file")?;

        info!("Logging config loaded from {:?}", path);
    } else {
        let stdout = ConsoleAppender::builder()
            .target(console::Target::Stdout)
            .build();

        let config = Config::builder()
            .appender(Appender::builder().build("stdout", Box::new(stdout)))
            .build(Root::builder().appender("stdout").build(LevelFilter::Info))
            .context("failed to construct logger")?;

        let _handle = log4rs::init_config(config).unwrap();

        warn!("Using default minimal logging config");
    }

    Ok(())
}
