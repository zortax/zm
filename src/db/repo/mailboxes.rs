use sqlx::SqlitePool;

use crate::error::Result;
use crate::state::mail::{Folder, FolderKind};

#[allow(dead_code)]
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DbMailbox {
    pub id: i64,
    pub account_id: String,
    pub name: String,
    pub delimiter: Option<String>,
    pub folder_kind: String,
}

pub async fn upsert(
    pool: &SqlitePool,
    account_id: &str,
    name: &str,
    delimiter: Option<&str>,
    folder_kind: &str,
) -> Result<()> {
    sqlx::query!(
        r#"INSERT INTO mailboxes (account_id, name, delimiter, folder_kind)
           VALUES (?, ?, ?, ?)
           ON CONFLICT(account_id, name) DO UPDATE
           SET delimiter = excluded.delimiter,
               folder_kind = excluded.folder_kind"#,
        account_id,
        name,
        delimiter,
        folder_kind,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list(pool: &SqlitePool, account_id: &str) -> Result<Vec<DbMailbox>> {
    let rows = sqlx::query_as!(
        DbMailbox,
        r#"SELECT id as "id!", account_id, name, delimiter, folder_kind
           FROM mailboxes
           WHERE account_id = ?
           ORDER BY
             CASE folder_kind
               WHEN 'inbox' THEN 0
               WHEN 'sent' THEN 1
               WHEN 'drafts' THEN 2
               WHEN 'trash' THEN 3
               WHEN 'archive' THEN 4
               ELSE 5
             END,
             name ASC"#,
        account_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn delete_stale(
    pool: &SqlitePool,
    account_id: &str,
    keep_names: &[String],
) -> Result<()> {
    if keep_names.is_empty() {
        return Ok(());
    }
    // Build placeholders for the IN clause
    let placeholders: Vec<&str> = keep_names.iter().map(|_| "?").collect();
    let in_clause = placeholders.join(", ");
    let sql = format!("DELETE FROM mailboxes WHERE account_id = ? AND name NOT IN ({in_clause})");

    let mut query = sqlx::query(&sql).bind(account_id);
    for name in keep_names {
        query = query.bind(name);
    }
    query.execute(pool).await?;
    Ok(())
}

pub async fn delete_by_account(pool: &SqlitePool, account_id: &str) -> Result<()> {
    sqlx::query!("DELETE FROM mailboxes WHERE account_id = ?", account_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Map common IMAP mailbox names to our FolderKind string representation.
pub fn folder_kind_from_name(name: &str) -> &'static str {
    match name.to_uppercase().as_str() {
        "INBOX" => "inbox",
        "SENT" | "SENT ITEMS" | "SENT MESSAGES" | "[GMAIL]/SENT MAIL" => "sent",
        "DRAFTS" | "[GMAIL]/DRAFTS" => "drafts",
        "TRASH" | "DELETED ITEMS" | "[GMAIL]/TRASH" | "[GMAIL]/BIN" => "trash",
        "ARCHIVE" | "ALL MAIL" | "[GMAIL]/ALL MAIL" => "archive",
        "JUNK" | "SPAM" | "[GMAIL]/SPAM" => "junk",
        _ => "custom",
    }
}

impl From<DbMailbox> for Folder {
    fn from(db: DbMailbox) -> Self {
        let kind = match db.folder_kind.as_str() {
            "inbox" => FolderKind::Inbox,
            "sent" => FolderKind::Sent,
            "drafts" => FolderKind::Drafts,
            "trash" => FolderKind::Trash,
            "archive" => FolderKind::Archive,
            _ => FolderKind::Custom(db.name.clone()),
        };
        Folder {
            kind,
            name: db.name,
            delimiter: db.delimiter,
            unread_count: 0, // populated separately
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn upsert_and_list() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        upsert(&pool, "acct1", "INBOX", Some("/"), "inbox")
            .await
            .unwrap();
        upsert(&pool, "acct1", "Sent", Some("/"), "sent")
            .await
            .unwrap();
        upsert(&pool, "acct1", "Custom", None, "custom")
            .await
            .unwrap();

        let mailboxes = list(&pool, "acct1").await.unwrap();
        assert_eq!(mailboxes.len(), 3);
        assert_eq!(mailboxes[0].name, "INBOX");
        assert_eq!(mailboxes[1].name, "Sent");
        assert_eq!(mailboxes[2].name, "Custom");
    }

    #[tokio::test]
    async fn upsert_is_idempotent() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        upsert(&pool, "acct1", "INBOX", Some("/"), "inbox")
            .await
            .unwrap();
        upsert(&pool, "acct1", "INBOX", Some("."), "inbox")
            .await
            .unwrap();

        let mailboxes = list(&pool, "acct1").await.unwrap();
        assert_eq!(mailboxes.len(), 1);
        assert_eq!(mailboxes[0].delimiter.as_deref(), Some("."));
    }

    #[tokio::test]
    async fn delete_stale_removes_old_mailboxes() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        upsert(&pool, "acct1", "INBOX", None, "inbox")
            .await
            .unwrap();
        upsert(&pool, "acct1", "Old", None, "custom").await.unwrap();
        upsert(&pool, "acct1", "Keep", None, "custom")
            .await
            .unwrap();

        delete_stale(&pool, "acct1", &["INBOX".into(), "Keep".into()])
            .await
            .unwrap();

        let mailboxes = list(&pool, "acct1").await.unwrap();
        assert_eq!(mailboxes.len(), 2);
        assert!(mailboxes.iter().all(|m| m.name != "Old"));
    }

    #[tokio::test]
    async fn accounts_are_isolated() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        upsert(&pool, "acct1", "INBOX", None, "inbox")
            .await
            .unwrap();
        upsert(&pool, "acct2", "INBOX", None, "inbox")
            .await
            .unwrap();
        upsert(&pool, "acct2", "Sent", None, "sent").await.unwrap();

        assert_eq!(list(&pool, "acct1").await.unwrap().len(), 1);
        assert_eq!(list(&pool, "acct2").await.unwrap().len(), 2);
    }

    #[test]
    fn folder_kind_mapping() {
        assert_eq!(folder_kind_from_name("INBOX"), "inbox");
        assert_eq!(folder_kind_from_name("Sent"), "sent");
        assert_eq!(folder_kind_from_name("Sent Items"), "sent");
        assert_eq!(folder_kind_from_name("[Gmail]/Sent Mail"), "sent");
        assert_eq!(folder_kind_from_name("Drafts"), "drafts");
        assert_eq!(folder_kind_from_name("Trash"), "trash");
        assert_eq!(folder_kind_from_name("Archive"), "archive");
        assert_eq!(folder_kind_from_name("My Folder"), "custom");
    }
}
