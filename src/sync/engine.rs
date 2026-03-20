use std::time::Duration;

use gpui::*;
use gpui_tokio::Tokio;
use sqlx::SqlitePool;

use crate::account::AccountManager;
use crate::db::repo::{mailboxes, messages};
use crate::sync::state::{SyncEvent, SyncStatus};

const BATCH_SIZE: usize = 50;

pub struct SyncEngine {
    account_id: String,
    pool: SqlitePool,
    status: SyncStatus,
    sync_task: Option<Task<()>>,
    _timer_task: Option<Task<()>>,
}

impl EventEmitter<SyncEvent> for SyncEngine {}

impl SyncEngine {
    pub fn new(
        account_id: String,
        pool: SqlitePool,
        sync_interval_secs: u64,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut engine = Self {
            account_id,
            pool,
            status: SyncStatus::Idle,
            sync_task: None,
            _timer_task: None,
        };

        engine.start_timer(sync_interval_secs, cx);
        engine.trigger_sync(cx);

        engine
    }

    pub fn status(&self) -> &SyncStatus {
        &self.status
    }

    pub fn is_syncing(&self) -> bool {
        self.status.is_active()
    }

    pub fn trigger_sync(&mut self, cx: &mut Context<Self>) {
        if self.is_syncing() {
            return;
        }

        let account_id = self.account_id.clone();
        let pool = self.pool.clone();

        self.set_status(SyncStatus::Connecting, cx);

        // Flume channel: tokio task sends progress, gpui task receives and updates entity
        let (tx, rx) = flume::unbounded::<SyncProgress>();

        // Spawn the tokio sync task
        Tokio::spawn(cx, {
            let account_id = account_id.clone();
            let pool = pool.clone();
            async move { run_sync(&account_id, &pool, tx).await }
        })
        .detach();

        // Spawn a gpui task that reads from the channel and updates the entity
        self.sync_task = Some(cx.spawn(async move |this, cx| {
            while let Ok(progress) = rx.recv_async().await {
                let should_break = this
                    .update(cx, |this, cx| match progress {
                        SyncProgress::Status(status) => {
                            this.set_status(status, cx);
                        }
                        SyncProgress::Done {
                            mailboxes,
                            messages,
                        } => {
                            let now = chrono::Local::now().format("%H:%M").to_string();
                            tracing::info!(mailboxes, messages, "sync completed");
                            this.set_status(SyncStatus::Completed { at: now }, cx);
                        }
                        SyncProgress::Failed { error } => {
                            tracing::error!(error, "sync failed");
                            this.set_status(SyncStatus::Failed { error }, cx);
                        }
                    })
                    .is_err();
                if should_break {
                    break;
                }
            }
        }));
    }

    fn set_status(&mut self, status: SyncStatus, cx: &mut Context<Self>) {
        self.status = status;
        cx.emit(SyncEvent);
        cx.notify();
    }

    fn start_timer(&mut self, interval_secs: u64, cx: &mut Context<Self>) {
        if interval_secs == 0 {
            return;
        }

        let duration = Duration::from_secs(interval_secs);
        let executor = cx.background_executor().clone();

        self._timer_task = Some(cx.spawn(async move |this, _cx| {
            loop {
                executor.timer(duration).await;
                let should_break = this
                    .update(_cx, |this, cx| {
                        this.trigger_sync(cx);
                    })
                    .is_err();
                if should_break {
                    break;
                }
            }
        }));
    }
}

enum SyncProgress {
    Status(SyncStatus),
    Done { mailboxes: usize, messages: usize },
    Failed { error: String },
}

async fn run_sync(account_id: &str, pool: &SqlitePool, tx: flume::Sender<SyncProgress>) {
    match run_sync_inner(account_id, pool, &tx).await {
        Ok((mb_count, msg_count)) => {
            let _ = tx.send(SyncProgress::Done {
                mailboxes: mb_count,
                messages: msg_count,
            });
        }
        Err(e) => {
            let _ = tx.send(SyncProgress::Failed {
                error: e.to_string(),
            });
        }
    }
}

async fn run_sync_inner(
    account_id: &str,
    pool: &SqlitePool,
    tx: &flume::Sender<SyncProgress>,
) -> crate::error::Result<(usize, usize)> {
    let mgr = AccountManager::load()?;
    let mut imap = mgr.imap_session(account_id).await?;

    // Sync mailboxes
    let _ = tx.send(SyncProgress::Status(SyncStatus::SyncingMailboxes));

    let imap_mailboxes = imap.list_mailboxes().await?;
    let mailbox_names: Vec<String> = imap_mailboxes.iter().map(|m| m.name.clone()).collect();

    for mb in &imap_mailboxes {
        let kind = mailboxes::folder_kind_from_name(&mb.name);
        mailboxes::upsert(pool, account_id, &mb.name, mb.delimiter.as_deref(), kind).await?;
    }
    mailboxes::delete_stale(pool, account_id, &mailbox_names).await?;

    // Notify that mailboxes are now in DB
    let _ = tx.send(SyncProgress::Status(SyncStatus::MailboxesSynced));

    let total_mailboxes = imap_mailboxes.len();
    let mut total_messages: usize = 0;

    // Sync messages for each mailbox
    for (mb_idx, mb) in imap_mailboxes.iter().enumerate() {
        let max_uid = messages::max_uid(pool, account_id, &mb.name).await?;

        let uids = if let Some(max) = max_uid {
            imap.fetch_new_uids(&mb.name, max as u32).await?
        } else {
            imap.fetch_uids(&mb.name).await?
        };

        if uids.is_empty() {
            let _ = tx.send(SyncProgress::Status(SyncStatus::SyncingMessages {
                mailbox: mb.name.clone(),
                fetched: 0,
                total_in_mailbox: 0,
                mailbox_index: mb_idx,
                mailbox_count: total_mailboxes,
            }));
            continue;
        }

        tracing::info!(
            account_id,
            mailbox = mb.name,
            new_messages = uids.len(),
            "fetching messages"
        );

        let total_in_mailbox = uids.len();
        let mut fetched_in_mailbox: usize = 0;

        // Fetch in batches
        for chunk in uids.chunks(BATCH_SIZE) {
            let fetched = imap.fetch_full_messages(chunk).await?;
            for msg in &fetched {
                let to_json = serde_json::to_string(&msg.to).unwrap_or_else(|_| "[]".into());
                let db_msg = messages::DbMessage {
                    id: 0,
                    account_id: account_id.into(),
                    mailbox_name: mb.name.clone(),
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
            fetched_in_mailbox += fetched.len();
            total_messages += fetched.len();

            // Report progress after each batch
            let _ = tx.send(SyncProgress::Status(SyncStatus::SyncingMessages {
                mailbox: mb.name.clone(),
                fetched: fetched_in_mailbox,
                total_in_mailbox,
                mailbox_index: mb_idx,
                mailbox_count: total_mailboxes,
            }));
        }
    }

    imap.logout().await?;

    Ok((total_mailboxes, total_messages))
}
