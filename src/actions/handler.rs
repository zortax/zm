use gpui::*;
use gpui_tokio::Tokio;
use sqlx::SqlitePool;

use crate::db::repo::messages;
use crate::state::mail::MailMessage;
use crate::state::mailbox::MailboxState;

use super::ops::{self, MessageTarget};

/// Emitted when the last in-flight IMAP action completes.
pub struct ActionsSettled;

pub struct MailActionHandler {
    mailbox: Entity<MailboxState>,
    pool: Option<SqlitePool>,
    in_flight: usize,
}

impl EventEmitter<ActionsSettled> for MailActionHandler {}

impl MailActionHandler {
    pub fn new(mailbox: Entity<MailboxState>) -> Self {
        Self {
            mailbox,
            pool: None,
            in_flight: 0,
        }
    }

    pub fn set_pool(&mut self, pool: SqlitePool) {
        self.pool = Some(pool);
    }

    /// Returns true if there are IMAP actions still in flight.
    pub fn has_pending_actions(&self) -> bool {
        self.in_flight > 0
    }

    pub fn toggle_read(&mut self, msg_index: usize, cx: &mut Context<Self>) {
        let Some(msg) = self.read_msg(msg_index, cx) else {
            return;
        };
        let new_read = !msg.is_read;
        let target = msg_target(&msg);
        let id = msg.id;

        self.mailbox.update(cx, |state, _| {
            state.toggle_read(msg_index);
        });

        if let Some(pool) = &self.pool {
            self.in_flight += 1;
            let pool = pool.clone();
            cx.spawn(async move |this, cx| {
                let db_result = Tokio::spawn(cx, {
                    let pool = pool.clone();
                    async move { messages::set_read(&pool, id, new_read).await }
                })
                .await;
                if let Err(e) = db_result {
                    tracing::error!(error = %e, "failed to update read status in db");
                }

                let imap_result =
                    Tokio::spawn(cx, async move { ops::set_read(&target, new_read).await }).await;
                if let Ok(Err(e)) = imap_result {
                    tracing::error!(error = %e, "failed to sync read status to IMAP");
                }

                let _ = this.update(cx, |this, cx| this.action_completed(cx));
            })
            .detach();
        }
    }

    pub fn mark_read(&mut self, msg_index: usize, cx: &mut Context<Self>) {
        let Some(msg) = self.read_msg(msg_index, cx) else {
            return;
        };
        if msg.is_read {
            return;
        }
        self.toggle_read(msg_index, cx);
    }

    pub fn toggle_star(&mut self, msg_index: usize, cx: &mut Context<Self>) {
        let Some(msg) = self.read_msg(msg_index, cx) else {
            return;
        };
        let new_starred = !msg.is_starred;
        let target = msg_target(&msg);
        let id = msg.id;

        self.mailbox.update(cx, |state, _| {
            state.toggle_star(msg_index);
        });

        if let Some(pool) = &self.pool {
            self.in_flight += 1;
            let pool = pool.clone();
            cx.spawn(async move |this, cx| {
                let db_result = Tokio::spawn(cx, {
                    let pool = pool.clone();
                    async move { messages::set_starred(&pool, id, new_starred).await }
                })
                .await;
                if let Err(e) = db_result {
                    tracing::error!(error = %e, "failed to update starred status in db");
                }

                let imap_result =
                    Tokio::spawn(
                        cx,
                        async move { ops::set_starred(&target, new_starred).await },
                    )
                    .await;
                if let Ok(Err(e)) = imap_result {
                    tracing::error!(error = %e, "failed to sync starred status to IMAP");
                }

                let _ = this.update(cx, |this, cx| this.action_completed(cx));
            })
            .detach();
        }
    }

    pub fn delete(&mut self, msg_index: usize, cx: &mut Context<Self>) {
        let Some(pool) = self.pool.clone() else {
            return;
        };

        let msg = self
            .mailbox
            .update(cx, |state, _| state.remove_message(msg_index));
        let Some(msg) = msg else {
            return;
        };

        let target = msg_target(&msg);
        let id = msg.id;
        let is_in_trash = self.is_trash_folder(cx);

        self.in_flight += 1;
        cx.spawn(async move |this, cx| {
            if is_in_trash {
                let db_result = Tokio::spawn(cx, {
                    let pool = pool.clone();
                    async move { messages::delete_by_id(&pool, id).await }
                })
                .await;
                if let Err(e) = db_result {
                    tracing::error!(error = %e, "failed to delete message from db");
                }

                let imap_result =
                    Tokio::spawn(cx, async move { ops::permanently_delete(&target).await }).await;
                if let Ok(Err(e)) = imap_result {
                    tracing::error!(error = %e, "failed to permanently delete on IMAP");
                }
            } else {
                let trash_folder = Tokio::spawn(cx, {
                    let pool = pool.clone();
                    let account_id = target.account_id.clone();
                    async move { messages::find_trash_folder(&pool, &account_id).await }
                })
                .await;

                let trash_name = match trash_folder {
                    Ok(Ok(Some(name))) => name,
                    _ => {
                        tracing::error!("no trash folder found for account");
                        let _ = this.update(cx, |this, cx| this.action_completed(cx));
                        return;
                    }
                };

                let db_result = Tokio::spawn(cx, {
                    let pool = pool.clone();
                    let trash = trash_name.clone();
                    async move { messages::move_to_mailbox(&pool, id, &trash).await }
                })
                .await;
                if let Err(e) = db_result {
                    tracing::error!(error = %e, "failed to move message to trash in db");
                }

                let imap_result = Tokio::spawn(cx, {
                    let trash = trash_name.clone();
                    async move { ops::move_to_trash(&target, &trash).await }
                })
                .await;
                if let Ok(Err(e)) = imap_result {
                    tracing::error!(error = %e, "failed to move message to trash on IMAP");
                }
            }

            let _ = this.update(cx, |this, cx| this.action_completed(cx));
        })
        .detach();
    }

    fn action_completed(&mut self, cx: &mut Context<Self>) {
        self.in_flight = self.in_flight.saturating_sub(1);
        if self.in_flight == 0 {
            cx.emit(ActionsSettled);
        }
    }

    fn read_msg(&self, index: usize, cx: &App) -> Option<MailMessage> {
        self.mailbox.read(cx).messages.get(index).cloned()
    }

    fn is_trash_folder(&self, cx: &App) -> bool {
        self.mailbox
            .read(cx)
            .active_folder()
            .map(|f| f.kind == crate::state::mail::FolderKind::Trash)
            .unwrap_or(false)
    }
}

fn msg_target(msg: &MailMessage) -> MessageTarget {
    MessageTarget {
        account_id: msg.account_id.clone(),
        mailbox_name: msg.mailbox_name.clone(),
        uid: msg.uid as u32,
    }
}
