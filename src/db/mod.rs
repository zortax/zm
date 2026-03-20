pub mod repo;

use std::path::PathBuf;

use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;

use crate::error::{Error, Result};

pub fn data_dir() -> Result<PathBuf> {
    dirs::data_dir()
        .map(|d| d.join("zm"))
        .ok_or_else(|| Error::Config("could not determine data directory".into()))
}

pub async fn connect() -> Result<SqlitePool> {
    let dir = data_dir()?;
    tokio::fs::create_dir_all(&dir).await?;

    let db_path = dir.join("zm.db");
    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

    let pool = SqlitePool::connect_with(options).await?;
    sqlx::migrate!().run(&pool).await?;

    tracing::info!(?db_path, "database connected");
    Ok(pool)
}
