use sqlx::SqlitePool;

use crate::db::repo::{mailboxes, messages};
use crate::state::mail::{Folder, MailMessage};

pub struct MailboxState {
    pub account_id: String,
    pub account_name: String,
    pub account_email: String,
    pub folders: Vec<Folder>,
    pub active_folder: usize,
    pub messages: Vec<MailMessage>,
    pub loading: bool,
    pool: Option<SqlitePool>,
}

impl MailboxState {
    pub fn new(account_id: String, account_name: String, account_email: String) -> Self {
        Self {
            account_id,
            account_name,
            account_email,
            folders: Vec::new(),
            active_folder: 0,
            messages: Vec::new(),
            loading: true,
            pool: None,
        }
    }

    pub fn pool(&self) -> Option<&SqlitePool> {
        self.pool.as_ref()
    }

    pub fn set_pool(&mut self, pool: SqlitePool) {
        self.pool = Some(pool);
    }

    pub fn active_folder(&self) -> Option<&Folder> {
        self.folders.get(self.active_folder)
    }

    /// Reload folders and current messages from the database.
    /// If `active_mailbox` is provided, loads messages for that folder;
    /// otherwise falls back to the first folder.
    pub async fn load_from_db(
        pool: &SqlitePool,
        account_id: &str,
        active_mailbox: Option<&str>,
    ) -> LoadedData {
        let db_mailboxes = mailboxes::list(pool, account_id).await.unwrap_or_default();

        let mut folders: Vec<Folder> = db_mailboxes.into_iter().map(Folder::from).collect();

        // Populate unread counts
        for folder in &mut folders {
            let unread = messages::count_unread(pool, account_id, &folder.name)
                .await
                .unwrap_or(0);
            folder.unread_count = unread as usize;
        }

        // Load messages for the active folder (or first folder as fallback)
        let target_mailbox = active_mailbox
            .and_then(|name| {
                folders
                    .iter()
                    .find(|f| f.name == name)
                    .map(|f| f.name.clone())
            })
            .or_else(|| folders.first().map(|f| f.name.clone()));

        let msgs = if let Some(ref name) = target_mailbox {
            messages::list(pool, account_id, name)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(MailMessage::from)
                .collect()
        } else {
            Vec::new()
        };

        LoadedData {
            folders,
            messages: msgs,
            active_folder_name: target_mailbox,
        }
    }

    /// Apply loaded data from the database, preserving the current folder selection.
    pub fn apply_loaded_data(&mut self, data: LoadedData) {
        // Use the folder name from LoadedData (which reflects what was actually loaded),
        // falling back to the current selection for incremental reloads.
        let active_name = data
            .active_folder_name
            .or_else(|| self.active_folder().map(|f| f.name.clone()));

        self.folders = data.folders;

        if let Some(ref name) = active_name {
            self.active_folder = self
                .folders
                .iter()
                .position(|f| f.name == *name)
                .unwrap_or(0);
        } else {
            self.active_folder = 0;
        }

        self.messages = data.messages;
        self.loading = false;
    }

    /// Refresh data for the current active folder from the database.
    pub async fn refresh_active_folder(
        pool: &SqlitePool,
        account_id: &str,
        mailbox_name: &str,
    ) -> Vec<MailMessage> {
        messages::list(pool, account_id, mailbox_name)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(MailMessage::from)
            .collect()
    }

    pub fn select_folder(&mut self, index: usize) {
        if index < self.folders.len() {
            self.active_folder = index;
            self.loading = true;
            self.messages.clear();
        }
    }

    pub fn switch_to(&mut self, account_id: String, account_name: String, account_email: String) {
        self.account_id = account_id;
        self.account_name = account_name;
        self.account_email = account_email;
        self.folders.clear();
        self.messages.clear();
        self.active_folder = 0;
        self.loading = true;
    }

    pub fn set_messages(&mut self, messages: Vec<MailMessage>) {
        self.messages = messages;
        self.loading = false;
    }

    pub fn toggle_star(&mut self, index: usize) {
        if let Some(msg) = self.messages.get_mut(index) {
            msg.is_starred = !msg.is_starred;
        }
    }

    pub fn toggle_read(&mut self, index: usize) {
        if let Some(msg) = self.messages.get_mut(index) {
            msg.is_read = !msg.is_read;
        }
    }

    pub fn remove_message(&mut self, index: usize) -> Option<MailMessage> {
        if index < self.messages.len() {
            Some(self.messages.remove(index))
        } else {
            None
        }
    }
}

pub struct LoadedData {
    pub folders: Vec<Folder>,
    pub messages: Vec<MailMessage>,
    pub active_folder_name: Option<String>,
}
