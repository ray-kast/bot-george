#[warn(missing_docs, clippy::all, clippy::pedantic, clippy::cargo)]
#[deny(broken_intra_doc_links, missing_debug_implementations)]
// TODO: maybe someday diesel won't rely on this??
#[macro_use]
extern crate diesel;
#[macro_use]
extern crate diesel_migrations;

mod config;
mod db;
pub mod error;
mod event_handler;
mod logging;
pub mod models;
pub mod schema;

use anyhow::Context;
use dotenv::dotenv;
use error::Result;
use event_handler::{Framework, Handler};
use futures::FutureExt;
use log::*;
use serenity::client::Client;
use std::{env, io};
use tokio::signal;

#[tokio::main]
async fn main() {
    match run().await {
        Ok(()) => (),
        Err(e) => {
            error!("Program terminated with error: {:?}", e);
            eprintln!("Program terminated with error: {:?}", e);
        },
    }
}

async fn run() -> Result<()> {
    // Show the MotD
    {
        use atty::Stream;
        use lazy_static::lazy_static;
        use regex::{Captures, Regex};

        lazy_static! {
            static ref FMT_REGEX: Regex = Regex::new(r"%\(([^\)]*)\)").unwrap();
        }

        println!(
            "{}",
            FMT_REGEX.replace_all(
                include_str!("motd.txt"),
                if atty::is(Stream::Stdout) {
                    |c: &Captures| format!("\x1b[{}m", c.get(1).unwrap().as_str())
                } else {
                    |_: &Captures| String::new()
                }
            )
        );
    }

    // Load .env
    let env_path = match dotenv() {
        Ok(p) => Some(p),
        Err(dotenv::Error::Io(e)) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e).context("failed to load .env"),
    };

    // Load logging config
    logging::init().context("failed to set up logging")?;

    // Now that logging is configured, log other basic info
    info!("This is BOT George v{}.", env!("CARGO_PKG_VERSION"));

    if let Some(path) = env_path {
        info!("Environment loaded from {:?}", path);
    }

    // Load configuration
    let conf = config::read().context("failed to load config")?;

    // Connect to the database
    let _db_conn = db::connect().context("failed to connect to the database")?;

    // Set up the API client
    let mut client = Client::new(&conf.auth.token)
        .event_handler(Handler)
        .framework(Framework)
        .await
        .context("failed to create Discord client")?;

    let shardman = client.shard_manager.clone();

    // Begin running, until client disconnects or program is interrupted
    tokio::select!(
        r = client
                .start()
                .map(|r| r.context("failed to start Discord client")) => {
            warn!("Client exited unexpectedly");

            r?;
        },
        r = signal::ctrl_c()
                .map(|r| r.map(|()| println!())
                .context("failed to handle SIGINT")) => {
            info!("SIGINT received, shutting down...");

            shardman.lock().await.shutdown_all().await;
            r?;
        },
    );

    Ok(())
}
