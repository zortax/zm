#[derive(Debug, Clone, PartialEq)]
pub enum SyncStatus {
    Idle,
    Connecting,
    SyncingMailboxes,
    /// Mailboxes are now stored in DB; folders can be shown.
    MailboxesSynced,
    SyncingMessages {
        mailbox: String,
        fetched: usize,
        total_in_mailbox: usize,
        mailbox_index: usize,
        mailbox_count: usize,
    },
    Completed {
        at: String,
    },
    Failed {
        error: String,
    },
}

impl SyncStatus {
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            SyncStatus::Connecting
                | SyncStatus::SyncingMailboxes
                | SyncStatus::MailboxesSynced
                | SyncStatus::SyncingMessages { .. }
        )
    }

    pub fn display(&self) -> String {
        match self {
            SyncStatus::Idle => "Ready".into(),
            SyncStatus::Connecting => "Connecting...".into(),
            SyncStatus::SyncingMailboxes => "Syncing mailboxes...".into(),
            SyncStatus::MailboxesSynced => "Loading messages...".into(),
            SyncStatus::SyncingMessages {
                mailbox,
                fetched,
                total_in_mailbox,
                mailbox_index,
                mailbox_count,
            } => {
                if *total_in_mailbox == 0 {
                    format!(
                        "Syncing [{}/{}] {mailbox}",
                        mailbox_index + 1,
                        mailbox_count
                    )
                } else {
                    format!(
                        "Syncing [{}/{}] {mailbox} ({fetched}/{total_in_mailbox})",
                        mailbox_index + 1,
                        mailbox_count
                    )
                }
            }
            SyncStatus::Completed { at } => format!("Synced {at}"),
            SyncStatus::Failed { error } => format!("Error: {error}"),
        }
    }

    /// Returns 0.0..1.0 progress for the overall sync, or None if not syncing.
    pub fn progress(&self) -> Option<f32> {
        match self {
            SyncStatus::Connecting => Some(0.0),
            SyncStatus::SyncingMailboxes => Some(0.02),
            SyncStatus::MailboxesSynced => Some(0.05),
            SyncStatus::SyncingMessages {
                fetched,
                total_in_mailbox,
                mailbox_index,
                mailbox_count,
                ..
            } => {
                if *mailbox_count == 0 {
                    return Some(0.5);
                }
                let mailbox_weight = 1.0 / *mailbox_count as f32;
                let base = *mailbox_index as f32 * mailbox_weight;
                let within = if *total_in_mailbox > 0 {
                    *fetched as f32 / *total_in_mailbox as f32
                } else {
                    1.0
                };
                // Reserve 5% for connecting/mailboxes, 95% for messages
                Some(0.05 + 0.95 * (base + within * mailbox_weight))
            }
            _ => None,
        }
    }
}

/// Emitted by SyncEngine whenever its state changes.
pub struct SyncEvent;

/// Emitted by IdleWatcher when the server reports changes in a mailbox.
pub struct IdleEvent {
    pub account_id: String,
    pub mailbox: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_active_when_syncing() {
        assert!(!SyncStatus::Idle.is_active());
        assert!(SyncStatus::Connecting.is_active());
        assert!(SyncStatus::SyncingMailboxes.is_active());
        assert!(SyncStatus::MailboxesSynced.is_active());
        assert!(
            SyncStatus::SyncingMessages {
                mailbox: "INBOX".into(),
                fetched: 0,
                total_in_mailbox: 10,
                mailbox_index: 0,
                mailbox_count: 3,
            }
            .is_active()
        );
        assert!(!SyncStatus::Completed { at: "now".into() }.is_active());
        assert!(
            !SyncStatus::Failed {
                error: "oops".into()
            }
            .is_active()
        );
    }

    #[test]
    fn progress_calculation() {
        assert_eq!(SyncStatus::Idle.progress(), None);
        assert_eq!(SyncStatus::Connecting.progress(), Some(0.0));

        let status = SyncStatus::SyncingMessages {
            mailbox: "INBOX".into(),
            fetched: 5,
            total_in_mailbox: 10,
            mailbox_index: 0,
            mailbox_count: 2,
        };
        let p = status.progress().unwrap();
        // First of 2 mailboxes, 50% done => 0.05 + 0.95 * (0 + 0.5 * 0.5) = 0.2875
        assert!((p - 0.2875).abs() < 0.001);
    }
}
