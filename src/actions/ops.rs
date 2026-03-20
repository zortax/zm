use sqlx::SqlitePool;

use crate::account::AccountManager;
use crate::db::repo::messages::{self, DbMessage};
use crate::error::{Error, Result};
use crate::mail::imap::ImapClient;

/// Identifies a message on the IMAP server.
pub struct MessageTarget {
    pub account_id: String,
    pub mailbox_name: String,
    pub uid: u32,
}

/// Set the \Seen flag on a message.
pub async fn set_read(target: &MessageTarget, is_read: bool) -> Result<()> {
    let mut imap = connect(&target.account_id).await?;
    imap.select(&target.mailbox_name).await?;
    if is_read {
        imap.add_flags(target.uid, "\\Seen").await?;
    } else {
        imap.remove_flags(target.uid, "\\Seen").await?;
    }
    imap.logout().await?;
    Ok(())
}

/// Set the \Flagged flag on a message.
pub async fn set_starred(target: &MessageTarget, is_starred: bool) -> Result<()> {
    let mut imap = connect(&target.account_id).await?;
    imap.select(&target.mailbox_name).await?;
    if is_starred {
        imap.add_flags(target.uid, "\\Flagged").await?;
    } else {
        imap.remove_flags(target.uid, "\\Flagged").await?;
    }
    imap.logout().await?;
    Ok(())
}

/// Move a message to the trash folder.
pub async fn move_to_trash(target: &MessageTarget, trash_folder: &str) -> Result<()> {
    let mut imap = connect(&target.account_id).await?;
    imap.select(&target.mailbox_name).await?;
    imap.move_message(target.uid, trash_folder).await?;
    imap.logout().await?;
    Ok(())
}

/// Permanently delete a message (for messages already in Trash).
pub async fn permanently_delete(target: &MessageTarget) -> Result<()> {
    let mut imap = connect(&target.account_id).await?;
    imap.select(&target.mailbox_name).await?;
    imap.delete_message(target.uid).await?;
    imap.logout().await?;
    Ok(())
}

/// Delete a draft from IMAP and the local database.
pub async fn delete_draft(
    account_id: &str,
    mailbox_name: &str,
    uid: u32,
    db_id: i64,
    pool: &SqlitePool,
) -> Result<()> {
    // Delete from IMAP
    let mut imap = connect(account_id).await?;
    imap.select(mailbox_name).await?;
    imap.delete_message(uid).await?;
    imap.logout().await?;

    // Delete from local DB
    messages::delete_by_id(pool, db_id).await?;
    Ok(())
}

/// Send a message via SMTP.
pub async fn send_message(account_id: &str, message: lettre::Message) -> Result<()> {
    let mgr = AccountManager::load()?;
    let smtp = mgr.smtp_client(account_id).await?;
    smtp.send(message).await?;
    Ok(())
}

/// Append a raw RFC 2822 message to the account's Drafts folder
/// and insert it into the local database for immediate display.
pub async fn append_to_drafts(
    account_id: &str,
    rfc822_bytes: Vec<u8>,
    pool: &SqlitePool,
) -> Result<()> {
    let drafts_folder = messages::find_drafts_folder(pool, account_id)
        .await?
        .ok_or_else(|| Error::Config("no Drafts folder found".into()))?;

    let mut imap = connect(account_id).await?;
    imap.append(&drafts_folder, &rfc822_bytes, Some("(\\Draft \\Seen)"))
        .await?;

    // Fetch the newly appended message's UID so we can store it locally
    // with the real server UID (avoids stale-UID deletion on next sync).
    let uids = imap.fetch_uids(&drafts_folder).await?;
    if let Some(&uid) = uids.iter().max() {
        let fetched = imap.fetch_full_messages(&[uid]).await?;
        if let Some(msg) = fetched.first() {
            let to_json = serde_json::to_string(&msg.to).unwrap_or_else(|_| "[]".into());
            let db_msg = DbMessage {
                id: 0,
                account_id: account_id.into(),
                mailbox_name: drafts_folder,
                uid: msg.uid as i64,
                subject: msg.subject.clone(),
                from_name: msg.from_name.clone(),
                from_email: msg.from_email.clone(),
                to_addresses: to_json,
                date: msg.date.clone(),
                body: msg.body.clone(),
                is_read: msg.is_read,
                is_starred: msg.is_starred,
                fetched_at: String::new(),
            };
            messages::upsert(pool, &db_msg).await?;
        }
    }

    imap.logout().await?;
    Ok(())
}

async fn connect(account_id: &str) -> Result<ImapClient> {
    let mgr = AccountManager::load()?;
    mgr.imap_session(account_id).await
}
