use sqlx::SqlitePool;

use crate::error::Result;

pub async fn get(pool: &SqlitePool, key: &str) -> Result<Option<String>> {
    let row = sqlx::query_scalar!(r#"SELECT value FROM ui_state WHERE key = ?"#, key,)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

pub async fn set(pool: &SqlitePool, key: &str, value: &str) -> Result<()> {
    sqlx::query!(
        r#"INSERT INTO ui_state (key, value) VALUES (?, ?)
           ON CONFLICT(key) DO UPDATE SET value = excluded.value"#,
        key,
        value,
    )
    .execute(pool)
    .await?;
    Ok(())
}
