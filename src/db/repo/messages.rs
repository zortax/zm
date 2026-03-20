use sqlx::SqlitePool;

use crate::error::Result;
use crate::state::mail::MailMessage;

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct DbMessage {
    pub id: i64,
    pub account_id: String,
    pub mailbox_name: String,
    pub uid: i64,
    pub subject: String,
    pub from_name: String,
    pub from_email: String,
    pub to_addresses: String, // JSON array
    pub date: String,
    pub body: String,
    pub is_read: bool,
    pub is_starred: bool,
    pub fetched_at: String,
}

pub async fn upsert(pool: &SqlitePool, msg: &DbMessage) -> Result<()> {
    sqlx::query!(
        r#"INSERT INTO messages (account_id, mailbox_name, uid, subject, from_name, from_email,
                                  to_addresses, date, body, is_read, is_starred)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
           ON CONFLICT(account_id, mailbox_name, uid) DO UPDATE
           SET subject = excluded.subject,
               from_name = excluded.from_name,
               from_email = excluded.from_email,
               to_addresses = excluded.to_addresses,
               date = excluded.date,
               body = excluded.body,
               is_read = excluded.is_read,
               is_starred = excluded.is_starred"#,
        msg.account_id,
        msg.mailbox_name,
        msg.uid,
        msg.subject,
        msg.from_name,
        msg.from_email,
        msg.to_addresses,
        msg.date,
        msg.body,
        msg.is_read,
        msg.is_starred,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list(
    pool: &SqlitePool,
    account_id: &str,
    mailbox_name: &str,
) -> Result<Vec<DbMessage>> {
    let rows = sqlx::query_as!(
        DbMessage,
        r#"SELECT id as "id!", account_id, mailbox_name, uid,
                  subject, from_name, from_email, to_addresses,
                  date, body, is_read as "is_read: bool",
                  is_starred as "is_starred: bool", fetched_at
           FROM messages
           WHERE account_id = ? AND mailbox_name = ?
           ORDER BY date DESC, uid DESC"#,
        account_id,
        mailbox_name,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn max_uid(
    pool: &SqlitePool,
    account_id: &str,
    mailbox_name: &str,
) -> Result<Option<i64>> {
    let row = sqlx::query!(
        r#"SELECT MAX(uid) as "max_uid: i64"
           FROM messages
           WHERE account_id = ? AND mailbox_name = ?"#,
        account_id,
        mailbox_name,
    )
    .fetch_one(pool)
    .await?;
    Ok(row.max_uid)
}

pub async fn count_unread(pool: &SqlitePool, account_id: &str, mailbox_name: &str) -> Result<i64> {
    let row = sqlx::query!(
        r#"SELECT COUNT(*) as "count!: i64"
           FROM messages
           WHERE account_id = ? AND mailbox_name = ? AND is_read = 0"#,
        account_id,
        mailbox_name,
    )
    .fetch_one(pool)
    .await?;
    Ok(row.count)
}

/// List all UIDs for a given account/mailbox.
pub async fn list_uids(
    pool: &SqlitePool,
    account_id: &str,
    mailbox_name: &str,
) -> Result<Vec<i64>> {
    let rows = sqlx::query!(
        r#"SELECT uid FROM messages WHERE account_id = ? AND mailbox_name = ?"#,
        account_id,
        mailbox_name,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.uid).collect())
}

/// Delete messages whose UIDs are NOT in the given set.
pub async fn delete_stale_uids(
    pool: &SqlitePool,
    account_id: &str,
    mailbox_name: &str,
    keep_uids: &[i64],
) -> Result<()> {
    if keep_uids.is_empty() {
        // Delete all messages in this mailbox
        sqlx::query!(
            "DELETE FROM messages WHERE account_id = ? AND mailbox_name = ?",
            account_id,
            mailbox_name,
        )
        .execute(pool)
        .await?;
        return Ok(());
    }
    let placeholders: Vec<&str> = keep_uids.iter().map(|_| "?").collect();
    let in_clause = placeholders.join(", ");
    let sql = format!(
        "DELETE FROM messages WHERE account_id = ? AND mailbox_name = ? AND uid NOT IN ({in_clause})"
    );
    let mut query = sqlx::query(&sql).bind(account_id).bind(mailbox_name);
    for uid in keep_uids {
        query = query.bind(uid);
    }
    query.execute(pool).await?;
    Ok(())
}

/// Update flags for an existing message identified by account/mailbox/uid.
pub async fn update_flags(
    pool: &SqlitePool,
    account_id: &str,
    mailbox_name: &str,
    uid: i64,
    is_read: bool,
    is_starred: bool,
) -> Result<()> {
    sqlx::query!(
        "UPDATE messages SET is_read = ?, is_starred = ? WHERE account_id = ? AND mailbox_name = ? AND uid = ?",
        is_read,
        is_starred,
        account_id,
        mailbox_name,
        uid,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_by_account(pool: &SqlitePool, account_id: &str) -> Result<()> {
    sqlx::query!("DELETE FROM messages WHERE account_id = ?", account_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_by_id(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query!("DELETE FROM messages WHERE id = ?", id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn move_to_mailbox(pool: &SqlitePool, id: i64, new_mailbox: &str) -> Result<()> {
    sqlx::query!(
        "UPDATE messages SET mailbox_name = ? WHERE id = ?",
        new_mailbox,
        id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn find_trash_folder(pool: &SqlitePool, account_id: &str) -> Result<Option<String>> {
    let row = sqlx::query!(
        r#"SELECT name FROM mailboxes WHERE account_id = ? AND folder_kind = 'trash' LIMIT 1"#,
        account_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.name))
}

pub async fn find_drafts_folder(pool: &SqlitePool, account_id: &str) -> Result<Option<String>> {
    let row = sqlx::query!(
        r#"SELECT name FROM mailboxes WHERE account_id = ? AND folder_kind = 'drafts' LIMIT 1"#,
        account_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.name))
}

pub async fn set_read(pool: &SqlitePool, id: i64, is_read: bool) -> Result<()> {
    sqlx::query!("UPDATE messages SET is_read = ? WHERE id = ?", is_read, id,)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_starred(pool: &SqlitePool, id: i64, is_starred: bool) -> Result<()> {
    sqlx::query!(
        "UPDATE messages SET is_starred = ? WHERE id = ?",
        is_starred,
        id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

impl From<DbMessage> for MailMessage {
    fn from(db: DbMessage) -> Self {
        let to: Vec<String> = serde_json::from_str(&db.to_addresses).unwrap_or_default();
        let date = format_date(&db.date);
        MailMessage {
            id: db.id,
            account_id: db.account_id,
            mailbox_name: db.mailbox_name,
            uid: db.uid,
            from_name: db.from_name,
            from_email: db.from_email,
            to,
            subject: db.subject,
            date,
            body: db.body,
            is_read: db.is_read,
            is_starred: db.is_starred,
        }
    }
}

/// Public wrapper for search result date formatting.
pub fn format_date_public(raw: &str) -> String {
    format_date(raw)
}

/// Format an RFC 3339 date string into a human-friendly local time display.
///
/// - Today: "10:30"
/// - Yesterday: "Yesterday"
/// - This year: "Mar 18"
/// - Older: "Mar 18, 2025"
/// - Unparseable: returned as-is
fn format_date(raw: &str) -> String {
    use chrono::{DateTime, Datelike, Local};

    let parsed = raw
        .parse::<DateTime<chrono::FixedOffset>>()
        .map(|dt| dt.with_timezone(&Local));

    let Ok(dt) = parsed else {
        return raw.to_string();
    };

    let now = Local::now();
    let today = now.date_naive();
    let msg_date = dt.date_naive();

    if msg_date == today {
        dt.format("%H:%M").to_string()
    } else if msg_date == today.pred_opt().unwrap_or(today) {
        "Yesterday".into()
    } else if dt.year() == now.year() {
        dt.format("%b %d").to_string()
    } else {
        dt.format("%b %d, %Y").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        pool
    }

    fn sample_message(account_id: &str, mailbox: &str, uid: i64) -> DbMessage {
        DbMessage {
            id: 0, // ignored on insert
            account_id: account_id.into(),
            mailbox_name: mailbox.into(),
            uid,
            subject: format!("Subject {uid}"),
            from_name: "Sender".into(),
            from_email: "sender@example.com".into(),
            to_addresses: r#"["recipient@example.com"]"#.into(),
            date: "2026-03-21".into(),
            body: "Hello world".into(),
            is_read: false,
            is_starred: false,
            fetched_at: String::new(),
        }
    }

    #[tokio::test]
    async fn upsert_and_list_messages() {
        let pool = test_pool().await;

        upsert(&pool, &sample_message("acct1", "INBOX", 1))
            .await
            .unwrap();
        upsert(&pool, &sample_message("acct1", "INBOX", 2))
            .await
            .unwrap();
        upsert(&pool, &sample_message("acct1", "Sent", 1))
            .await
            .unwrap();

        let inbox = list(&pool, "acct1", "INBOX").await.unwrap();
        assert_eq!(inbox.len(), 2);
        // Ordered by uid DESC
        assert_eq!(inbox[0].uid, 2);
        assert_eq!(inbox[1].uid, 1);

        let sent = list(&pool, "acct1", "Sent").await.unwrap();
        assert_eq!(sent.len(), 1);
    }

    #[tokio::test]
    async fn upsert_updates_existing() {
        let pool = test_pool().await;

        let mut msg = sample_message("acct1", "INBOX", 1);
        upsert(&pool, &msg).await.unwrap();

        msg.subject = "Updated subject".into();
        upsert(&pool, &msg).await.unwrap();

        let messages = list(&pool, "acct1", "INBOX").await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].subject, "Updated subject");
    }

    #[tokio::test]
    async fn max_uid_returns_highest() {
        let pool = test_pool().await;

        assert_eq!(max_uid(&pool, "acct1", "INBOX").await.unwrap(), None);

        upsert(&pool, &sample_message("acct1", "INBOX", 5))
            .await
            .unwrap();
        upsert(&pool, &sample_message("acct1", "INBOX", 10))
            .await
            .unwrap();
        upsert(&pool, &sample_message("acct1", "INBOX", 3))
            .await
            .unwrap();

        assert_eq!(max_uid(&pool, "acct1", "INBOX").await.unwrap(), Some(10));
    }

    #[tokio::test]
    async fn count_unread_messages() {
        let pool = test_pool().await;

        upsert(&pool, &sample_message("acct1", "INBOX", 1))
            .await
            .unwrap();
        upsert(&pool, &sample_message("acct1", "INBOX", 2))
            .await
            .unwrap();

        let mut read_msg = sample_message("acct1", "INBOX", 3);
        read_msg.is_read = true;
        upsert(&pool, &read_msg).await.unwrap();

        assert_eq!(count_unread(&pool, "acct1", "INBOX").await.unwrap(), 2);
    }

    #[tokio::test]
    async fn set_read_and_starred() {
        let pool = test_pool().await;

        upsert(&pool, &sample_message("acct1", "INBOX", 1))
            .await
            .unwrap();
        let messages = list(&pool, "acct1", "INBOX").await.unwrap();
        let id = messages[0].id;

        set_read(&pool, id, true).await.unwrap();
        set_starred(&pool, id, true).await.unwrap();

        let messages = list(&pool, "acct1", "INBOX").await.unwrap();
        assert!(messages[0].is_read);
        assert!(messages[0].is_starred);
    }

    #[tokio::test]
    async fn accounts_are_isolated() {
        let pool = test_pool().await;

        upsert(&pool, &sample_message("acct1", "INBOX", 1))
            .await
            .unwrap();
        upsert(&pool, &sample_message("acct2", "INBOX", 1))
            .await
            .unwrap();

        assert_eq!(list(&pool, "acct1", "INBOX").await.unwrap().len(), 1);
        assert_eq!(list(&pool, "acct2", "INBOX").await.unwrap().len(), 1);
    }
}
