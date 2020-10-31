#![warn(missing_docs, clippy::all, clippy::pedantic)]
#![deny(broken_intra_doc_links, missing_debug_implementations)]
#![allow(clippy::module_name_repetitions)]
#![feature(async_closure)]

//! Companion bot for the UCSB GDC Discord server

// TODO: maybe someday diesel won't rely on this??
#[macro_use]
extern crate diesel;
#[macro_use]
extern crate diesel_migrations;

mod bot;
pub mod commands;
mod config;
mod db;
pub mod error;
mod event_handler;
mod logging;
pub mod models;
#[allow(missing_docs)]
pub mod schema;
pub mod util;

use anyhow::Context;
use dotenv::dotenv;
use error::Result;
use event_handler::Handler;
use futures::FutureExt;
use lazy_static::lazy_static;
use log::{error, info, warn};
use serenity::{client::Client, model::id::UserId};
use std::{
    env, io, panic,
    sync::atomic::{AtomicBool, Ordering},
};
use tokio::{runtime, signal};

lazy_static! {
    static ref HAS_LOGGING: AtomicBool = AtomicBool::new(false);
}

fn main() {
    let panic_hook = panic::take_hook();
    panic::set_hook(Box::new(move |i| {
        error!("Worker thread panicked: {:#}", i);

        panic_hook(i);
    }));

    runtime::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .thread_name("bot-george-worker")
        .build()
        .unwrap()
        .block_on(main_async());
}

async fn main_async() {
    match run().await {
        Ok(()) => (),
        Err(e) => {
            error!("Program terminated with error: {:?}", e);

            if !HAS_LOGGING.load(Ordering::Relaxed) {
                eprintln!("Program terminated with error: {:?}", e);
            }
        },
    }
}

async fn run() -> Result<()> {
    // Show the MotD
    {
        use atty::Stream;
        use regex::{Captures, Regex};

        lazy_static! {
            static ref FMT_REGEX: Regex = Regex::new(r"%\(([^\)]*)\)").unwrap();
        }

        println!(
            "{}",
            FMT_REGEX.replace_all(
                include_str!("motd.txt"),
                if atty::is(Stream::Stdout) {
                    |c: &Captures| format!("\x1b[{}m", &c[1])
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
    HAS_LOGGING.store(true, Ordering::Relaxed);

    // Now that logging is configured, log other basic info
    info!("This is BOT George v{}.", env!("CARGO_PKG_VERSION"));

    if let Some(path) = env_path {
        info!("Environment loaded from {:?}", path);
    }

    // Load configuration
    let conf = config::read().context("failed to load config")?;

    // Connect to the database
    let db_conn = db::connect().context("failed to connect to the database")?;

    // Set up the API client
    let handler = Handler::new(
        conf.general.command_prefix,
        UserId(conf.auth.superuser),
        db_conn,
    )?;
    let mut client = Client::new(&conf.auth.token)
        .event_handler(handler)
        .await
        .context("failed to create Discord client")?;

    let shard_man = client.shard_manager.clone();

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

            shard_man.lock().await.shutdown_all().await;
            r?;
        },
    );

    Ok(())
}
