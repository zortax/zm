use std::sync::Mutex;
use std::time::Duration;

use async_imap::extensions::idle::IdleResponse;
use gpui::*;
use gpui_tokio::Tokio;
use sqlx::SqlitePool;

use crate::account::AccountManager;
use crate::db::repo::messages;
use crate::mail::imap::{ImapClient, ImapSession};
use crate::sync::state::IdleEvent;

pub struct IdleWatcher {
    account_id: String,
    watching_mailbox: Option<String>,
    cmd_tx: Option<flume::Sender<IdleCommand>>,
    _watcher_task: Option<Task<()>>,
}

impl EventEmitter<IdleEvent> for IdleWatcher {}

enum IdleCommand {
    SwitchMailbox(String),
    Shutdown,
}

enum IdleNotification {
    Changed { mailbox: String },
    Disconnected { error: String },
    Reconnected,
}

impl IdleWatcher {
    pub fn new(
        account_id: String,
        pool: SqlitePool,
        initial_mailbox: String,
        cx: &mut Context<Self>,
    ) -> Self {
        let (cmd_tx, cmd_rx) = flume::unbounded::<IdleCommand>();
        let (notify_tx, notify_rx) = flume::unbounded::<IdleNotification>();

        Tokio::spawn(cx, {
            let account_id = account_id.clone();
            let pool = pool.clone();
            let mailbox = initial_mailbox.clone();
            async move {
                run_idle_loop(&account_id, &pool, mailbox, cmd_rx, notify_tx).await;
            }
        })
        .detach();

        let watcher_task = cx.spawn(async move |this, cx| {
            while let Ok(notification) = notify_rx.recv_async().await {
                let should_break = this
                    .update(cx, |this, cx| match notification {
                        IdleNotification::Changed { mailbox } => {
                            cx.emit(IdleEvent {
                                account_id: this.account_id.clone(),
                                mailbox,
                            });
                        }
                        IdleNotification::Disconnected { error } => {
                            tracing::warn!(
                                error,
                                account = %this.account_id,
                                "IDLE disconnected, will reconnect"
                            );
                        }
                        IdleNotification::Reconnected => {
                            tracing::info!(account = %this.account_id, "IDLE reconnected");
                        }
                    })
                    .is_err();
                if should_break {
                    break;
                }
            }
        });

        Self {
            account_id,
            watching_mailbox: Some(initial_mailbox),
            cmd_tx: Some(cmd_tx),
            _watcher_task: Some(watcher_task),
        }
    }

    pub fn watch_mailbox(&mut self, mailbox: String) {
        if self.watching_mailbox.as_ref() == Some(&mailbox) {
            return;
        }
        self.watching_mailbox = Some(mailbox.clone());
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(IdleCommand::SwitchMailbox(mailbox));
        }
    }
}

impl Drop for IdleWatcher {
    fn drop(&mut self) {
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(IdleCommand::Shutdown);
        }
    }
}

async fn run_idle_loop(
    account_id: &str,
    pool: &SqlitePool,
    initial_mailbox: String,
    cmd_rx: flume::Receiver<IdleCommand>,
    notify_tx: flume::Sender<IdleNotification>,
) {
    let mut current_mailbox = initial_mailbox;
    let mut backoff_secs = 5u64;
    let pending_cmd: std::sync::Arc<Mutex<Option<IdleCommand>>> =
        std::sync::Arc::new(Mutex::new(None));

    'outer: loop {
        // Connect
        let session = match connect(account_id).await {
            Ok(session) => session,
            Err(e) => {
                let _ = notify_tx.send(IdleNotification::Disconnected {
                    error: e.to_string(),
                });
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(300);
                continue;
            }
        };

        let mut session = session.into_session();

        if let Err(e) = session.select(&current_mailbox).await {
            tracing::warn!(error = %e, mailbox = %current_mailbox, "IDLE select failed");
            let _ = notify_tx.send(IdleNotification::Disconnected {
                error: e.to_string(),
            });
            tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
            backoff_secs = (backoff_secs * 2).min(300);
            continue;
        }

        backoff_secs = 5;
        let _ = notify_tx.send(IdleNotification::Reconnected);

        'idle: loop {
            // Drain pending commands before entering IDLE
            while let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    IdleCommand::SwitchMailbox(mb) => {
                        current_mailbox = mb;
                        if let Err(_) = session.select(&current_mailbox).await {
                            break 'idle;
                        }
                    }
                    IdleCommand::Shutdown => break 'outer,
                }
            }

            let mut handle = session.idle();
            if let Err(e) = handle.init().await {
                tracing::warn!(error = %e, "IDLE init failed");
                // Try to recover session for reconnect
                match handle.done().await {
                    Ok(s) => session = s,
                    Err(_) => {}
                }
                break 'idle;
            }

            let (idle_fut, stop_source) = handle.wait_with_timeout(Duration::from_secs(28 * 60));

            // Spawn a helper task that listens for commands and interrupts IDLE
            // by dropping the stop_source.
            let cmd_rx_clone = cmd_rx.clone();
            let pending_clone = pending_cmd.clone();
            let interrupt_task = tokio::spawn(async move {
                if let Ok(cmd) = cmd_rx_clone.recv_async().await {
                    *pending_clone.lock().unwrap() = Some(cmd);
                    drop(stop_source);
                }
            });

            let result = idle_fut.await;
            interrupt_task.abort();

            session = match handle.done().await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(error = %e, "IDLE done failed");
                    break 'idle;
                }
            };

            match result {
                Ok(IdleResponse::NewData(_data)) => {
                    // Do incremental sync on the same connection
                    if let Err(e) =
                        sync_mailbox_after_idle(&mut session, pool, account_id, &current_mailbox)
                            .await
                    {
                        tracing::warn!(error = %e, "incremental sync after IDLE failed");
                        break 'idle;
                    }
                    let _ = notify_tx.send(IdleNotification::Changed {
                        mailbox: current_mailbox.clone(),
                    });
                }
                Ok(IdleResponse::Timeout) => {
                    // Normal timeout, re-issue IDLE
                }
                Ok(IdleResponse::ManualInterrupt) => {
                    let cmd = pending_cmd.lock().unwrap().take();
                    match cmd {
                        Some(IdleCommand::SwitchMailbox(mb)) => {
                            current_mailbox = mb;
                            if let Err(_) = session.select(&current_mailbox).await {
                                break 'idle;
                            }
                        }
                        Some(IdleCommand::Shutdown) | None => break 'outer,
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "IDLE wait error");
                    break 'idle;
                }
            }
        }

        // Connection lost, reconnect with backoff
        let _ = notify_tx.send(IdleNotification::Disconnected {
            error: "connection lost".into(),
        });
        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(300);
    }
}

/// Full mailbox sync after IDLE notification.
///
/// Handles all three cases:
/// - Flag changes on existing messages (re-fetch flags, update DB)
/// - Deleted messages (UIDs gone from server, remove from DB)
/// - New messages (fetch full body + flags, insert into DB)
async fn sync_mailbox_after_idle(
    session: &mut ImapSession,
    pool: &SqlitePool,
    account_id: &str,
    mailbox_name: &str,
) -> crate::error::Result<()> {
    use crate::db::repo::messages::DbMessage;
    use futures::TryStreamExt;
    use std::collections::HashSet;

    // 1. Get all UIDs currently on the server
    let server_uids: Vec<u32> = session.uid_search("ALL").await?.into_iter().collect();
    let server_uid_set: HashSet<i64> = server_uids.iter().map(|&u| u as i64).collect();

    // 2. Get all UIDs we have locally
    let local_uids = messages::list_uids(pool, account_id, mailbox_name).await?;
    let local_uid_set: HashSet<i64> = local_uids.into_iter().collect();

    // 3. Delete messages that no longer exist on server
    let keep_uids: Vec<i64> = server_uid_set.iter().copied().collect();
    messages::delete_stale_uids(pool, account_id, mailbox_name, &keep_uids).await?;

    // 4. Find new UIDs (on server but not local)
    let new_uids: Vec<u32> = server_uids
        .iter()
        .filter(|&&uid| !local_uid_set.contains(&(uid as i64)))
        .copied()
        .collect();

    // 5. Find existing UIDs (on both server and local) — need flag refresh
    let existing_uids: Vec<u32> = server_uids
        .iter()
        .filter(|&&uid| local_uid_set.contains(&(uid as i64)))
        .copied()
        .collect();

    // 6. Re-fetch flags for existing messages
    if !existing_uids.is_empty() {
        for chunk in existing_uids.chunks(200) {
            let uid_set: String = chunk
                .iter()
                .map(|u| u.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let fetch_data: Vec<_> = session
                .uid_fetch(&uid_set, "(UID FLAGS)")
                .await?
                .try_collect()
                .await?;

            for msg in &fetch_data {
                let Some(uid) = msg.uid else { continue };
                let is_read = msg
                    .flags()
                    .any(|f| matches!(f, async_imap::types::Flag::Seen));
                let is_starred = msg
                    .flags()
                    .any(|f| matches!(f, async_imap::types::Flag::Flagged));
                messages::update_flags(
                    pool,
                    account_id,
                    mailbox_name,
                    uid as i64,
                    is_read,
                    is_starred,
                )
                .await?;
            }
        }
    }

    // 7. Fetch full messages for new UIDs
    if !new_uids.is_empty() {
        tracing::info!(
            account_id,
            mailbox_name,
            new_messages = new_uids.len(),
            "IDLE: fetching new messages"
        );

        for chunk in new_uids.chunks(50) {
            let uid_set: String = chunk
                .iter()
                .map(|u| u.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let fetch_data: Vec<_> = session
                .uid_fetch(&uid_set, "(UID FLAGS BODY.PEEK[])")
                .await?
                .try_collect()
                .await?;

            for msg in &fetch_data {
                let Some(uid) = msg.uid else { continue };
                let Some(body_bytes) = msg.body() else {
                    continue;
                };

                let is_read = msg
                    .flags()
                    .any(|f| matches!(f, async_imap::types::Flag::Seen));
                let is_starred = msg
                    .flags()
                    .any(|f| matches!(f, async_imap::types::Flag::Flagged));

                let parsed = mail_parser::MessageParser::default().parse(body_bytes);
                let Some(ref parsed_msg) = parsed else {
                    tracing::warn!(uid, "failed to parse message body during IDLE sync");
                    continue;
                };

                let subject = parsed_msg.subject().unwrap_or("").to_string();
                let (from_name, from_email) = parsed_msg
                    .from()
                    .and_then(|f| f.first())
                    .map(|addr| {
                        (
                            addr.name().unwrap_or("").to_string(),
                            addr.address().unwrap_or("").to_string(),
                        )
                    })
                    .unwrap_or_default();

                let to: Vec<String> = parsed_msg
                    .to()
                    .map(|list| {
                        list.iter()
                            .filter_map(|a| a.address().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                let date = parsed_msg
                    .date()
                    .map(|d| crate::mail::imap::date_to_utc_rfc3339(d))
                    .unwrap_or_default();

                let body_text = parsed_msg.body_text(0).unwrap_or_default().to_string();
                let to_json = serde_json::to_string(&to).unwrap_or_else(|_| "[]".into());

                let db_msg = DbMessage {
                    id: 0,
                    account_id: account_id.into(),
                    mailbox_name: mailbox_name.into(),
                    uid: uid as i64,
                    subject,
                    from_name,
                    from_email,
                    to_addresses: to_json,
                    date,
                    body: body_text,
                    is_read,
                    is_starred,
                    fetched_at: String::new(),
                };
                messages::upsert(pool, &db_msg).await?;
            }
        }
    }

    Ok(())
}

async fn connect(account_id: &str) -> crate::error::Result<ImapClient> {
    let mgr = AccountManager::load()?;
    mgr.imap_session(account_id).await
}
