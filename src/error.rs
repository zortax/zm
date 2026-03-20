use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("config: {0}")]
    Config(String),

    #[error("credential: {0}")]
    Credential(String),

    #[error("imap: {0}")]
    Imap(#[from] async_imap::error::Error),

    #[error("smtp: {0}")]
    Smtp(#[from] lettre::transport::smtp::Error),

    #[error("message: {0}")]
    Message(#[from] lettre::error::Error),

    #[error("address: {0}")]
    Address(#[from] lettre::address::AddressError),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("toml deserialize: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("toml serialize: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("database: {0}")]
    Db(#[from] sqlx::Error),

    #[error("migration: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
