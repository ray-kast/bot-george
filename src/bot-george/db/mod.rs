pub mod models;

use crate::error::Result;
use anyhow::Context;
use diesel::{
    pg::PgConnection,
    r2d2::{ConnectionManager, Pool, PooledConnection},
};
use log::{debug, info, warn};
use std::env;

embed_migrations!("../../migrations");

pub type DbConnection = PgConnection;
pub type DbConnectionManager = ConnectionManager<DbConnection>;
pub type DbPool = Pool<DbConnectionManager>;
pub type _DbPooledConnection = PooledConnection<DbConnectionManager>;

pub fn connect() -> Result<DbPool> {
    let db_url = env::var("DATABASE_URL").context("failed to acquire database URL")?;
    debug!("Connecting to database at {:?}...", db_url);

    let man = DbConnectionManager::new(db_url);
    // TODO: config for this?
    let pool = DbPool::builder()
        .build(man)
        .context("failed to create database connection pool")?;

    let mut out = vec![];

    info!("Running database migrations...");
    embedded_migrations::run_with_output(
        &pool.get().context("failed to connect to database")?,
        &mut out,
    )
    .context("failed to run database migrations")?;

    match std::str::from_utf8(&out) {
        Ok(s) => {
            let s = s.trim();

            if !s.is_empty() {
                info!("Output from migrations:\n{}", s);
            }
        },
        Err(e) => warn!("Failed to read migration output: {}", e),
    }

    Ok(pool)
}
