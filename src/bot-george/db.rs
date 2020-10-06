use crate::error::Result;
use anyhow::Context;
use diesel::{pg::PgConnection, prelude::*};
use log::*;
use std::env;

embed_migrations!("../../migrations");

pub fn connect() -> Result<PgConnection> {
    let db_url = env::var("DATABASE_URL").context("failed to acquire database URL")?;
    debug!("Connecting to database at {:?}...", db_url);

    let conn = PgConnection::establish(&db_url).context("failed to connect to database")?;

    let mut out = vec![];

    info!("Running database migrations...");
    embedded_migrations::run_with_output(&conn, &mut out)
        .context("failed to run database migrations")?;

    match std::str::from_utf8(&out) {
        Ok(s) => {
            let s = s.trim();

            if s.len() > 0 {
                info!("Output from migrations:\n{}", s);
            }
        },
        Err(e) => warn!("Failed to read migration output: {}", e),
    }

    Ok(conn)
}
