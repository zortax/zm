mod sidebar;

use std::collections::HashMap;
use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme, Side, Sizable, WindowExt as _, h_flex,
    list::{List, ListState},
    sidebar::SidebarToggleButton,
    v_flex,
};
use gpui_tokio::Tokio;

use crate::actions::{ActionsSettled, MailActionHandler};
use crate::assets::IconName;
use crate::components::status_bar::StatusBar;
use crate::config::{self, AccountConfig};
use crate::db;
use crate::db::repo::{mailboxes, messages, ui_state};
use crate::search::engine::{SearchEngine, SearchEngineEvent, SearchEngineStatus};
use crate::search::store::SearchResult;
use crate::state::mail::MailMessage;
use crate::state::mailbox::MailboxState;
use crate::sync::engine::SyncEngine;
use crate::sync::idle::IdleWatcher;
use crate::sync::state::{IdleEvent, SyncEvent, SyncStatus};
use crate::views::compose::{ComposeEvent, ComposeView};
use crate::views::mail_list::MailListDelegate;
use crate::views::search::SearchListDelegate;
use crate::views::settings::{SettingsEvent, SettingsView};
use crate::views::setup_wizard::{AccountSaved, SetupWizard};
use gpui_component::input::{Input, InputEvent, InputState};

pub struct NoAccountsRemaining;
impl EventEmitter<NoAccountsRemaining> for InboxView {}

/// Navigation state preserved across folder and account switches.
struct SavedNavState {
    selected_index: Option<gpui_component::IndexPath>,
    scroll_offset: Point<Pixels>,
}

/// Key for per-folder navigation cache: (account_id, folder_name).
type NavCacheKey = (String, String);

pub struct InboxView {
    mailbox: Entity<MailboxState>,
    action_handler: Entity<MailActionHandler>,
    mail_list: Entity<ListState<MailListDelegate>>,
    sync_engines: HashMap<String, Entity<SyncEngine>>,
    idle_watchers: HashMap<String, Entity<IdleWatcher>>,
    accounts: Vec<AccountConfig>,
    active_account_id: String,
    pool: Option<sqlx::SqlitePool>,
    nav_cache: HashMap<NavCacheKey, SavedNavState>,
    /// Remembers last active folder per account for account switching.
    last_folder: HashMap<String, String>,
    collapsed: bool,
    // Semantic search
    search_engine: Option<Entity<SearchEngine>>,
    search_input: Entity<InputState>,
    search_list: Entity<ListState<SearchListDelegate>>,
    search_query: String,
    search_results: Vec<SearchResult>,
    is_searching: bool,
    _search_debounce: Option<Task<()>>,
    /// Folder that was active before search started (to restore on clear).
    pre_search_folder: Option<usize>,
    _subscriptions: Vec<Subscription>,
    _init_task: Option<Task<()>>,
}

impl InboxView {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let app_config = config::load().unwrap_or_default();
        let accounts = app_config.accounts.clone();
        // Start with the first account; the init task will restore the persisted
        // active account from the DB once the pool is ready.
        let account = accounts.first().cloned();

        let (account_id, account_name, account_email) = if let Some(ref acct) = account {
            (
                acct.id.clone(),
                acct.display_name.clone(),
                acct.email.clone(),
            )
        } else {
            (String::new(), "No Account".into(), String::new())
        };

        let mailbox =
            cx.new(|_| MailboxState::new(account_id.clone(), account_name, account_email));

        let action_handler = cx.new({
            let mailbox = mailbox.clone();
            |_| MailActionHandler::new(mailbox)
        });

        let mail_list = cx.new({
            let mailbox = mailbox.clone();
            let handler = action_handler.clone();
            |cx| {
                ListState::new(MailListDelegate::new(mailbox, handler), _window, cx)
                    .searchable(true)
            }
        });

        let on_open_draft: Rc<dyn Fn(MailMessage, &mut Window, &mut App)> = {
            let listener = cx.listener(|this, msg: &MailMessage, window, cx| {
                this.open_draft(msg, window, cx);
            });
            Rc::new(move |msg, window, cx| {
                listener(&msg, window, cx);
            })
        };
        mail_list.update(cx, |list, _cx| {
            list.delegate_mut().on_open_draft = Some(on_open_draft);
        });

        let search_list = cx.new(|cx| ListState::new(SearchListDelegate::new(), _window, cx));

        let search_input =
            cx.new(|cx| InputState::new(_window, cx).placeholder("Search all mail..."));

        let mut view = Self {
            mailbox: mailbox.clone(),
            action_handler: action_handler.clone(),
            mail_list,
            sync_engines: HashMap::new(),
            idle_watchers: HashMap::new(),
            accounts,
            active_account_id: account_id.clone(),
            pool: None,
            nav_cache: HashMap::new(),
            last_folder: HashMap::new(),
            collapsed: false,
            search_engine: None,
            search_input: search_input.clone(),
            search_list,
            search_query: String::new(),
            search_results: Vec::new(),
            is_searching: false,
            _search_debounce: None,
            pre_search_folder: None,
            _subscriptions: vec![
                cx.subscribe(&action_handler, Self::on_actions_settled),
                cx.subscribe(&search_input, Self::on_search_input_event),
            ],
            _init_task: None,
        };

        if !view.accounts.is_empty() {
            let mailbox_handle = mailbox.clone();
            let accounts_for_init = view.accounts.clone();

            view._init_task = Some(cx.spawn(async move |this, cx| {
                let pool_result = Tokio::spawn(cx, async move { db::connect().await }).await;

                let pool = match pool_result {
                    Ok(Ok(pool)) => pool,
                    Ok(Err(e)) => {
                        tracing::error!(error = %e, "failed to connect to database");
                        return;
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "database init task failed");
                        return;
                    }
                };

                // Check if a different account was persisted as active
                let pool_for_active = pool.clone();
                let persisted_account = Tokio::spawn(cx, async move {
                    ui_state::get(&pool_for_active, "active_account").await
                })
                .await;

                let active_id = persisted_account
                    .ok()
                    .and_then(|r| r.ok())
                    .flatten()
                    .filter(|id| accounts_for_init.iter().any(|a| a.id == *id))
                    .unwrap_or(account_id.clone());

                // Load data for the active account
                let pool_for_load = pool.clone();
                let active_id_for_load = active_id.clone();
                let loaded = Tokio::spawn(cx, async move {
                    MailboxState::load_from_db(&pool_for_load, &active_id_for_load, None).await
                })
                .await;

                this.update(cx, |this, cx| {
                    this.pool = Some(pool.clone());

                    // If persisted account differs from the default, switch to it
                    if active_id != this.active_account_id {
                        if let Some(acct) =
                            this.accounts.iter().find(|a| a.id == active_id).cloned()
                        {
                            this.active_account_id = acct.id.clone();
                            this.mailbox.update(cx, |state, _| {
                                state.switch_to(
                                    acct.id.clone(),
                                    acct.display_name.clone(),
                                    acct.email.clone(),
                                );
                            });
                        }
                    }

                    this.action_handler.update(cx, |handler, _| {
                        handler.set_pool(pool.clone());
                    });

                    mailbox_handle.update(cx, |state, _| {
                        state.set_pool(pool.clone());
                        if let Ok(data) = loaded {
                            state.apply_loaded_data(data);
                        }
                    });

                    // Create a SyncEngine and IdleWatcher for every account
                    for acct in &accounts_for_init {
                        let engine = cx.new({
                            let id = acct.id.clone();
                            let p = pool.clone();
                            let interval = acct.sync_interval_secs;
                            move |cx| SyncEngine::new(id, p, interval, cx)
                        });

                        this._subscriptions
                            .push(cx.subscribe(&engine, Self::on_sync_event));
                        this.sync_engines.insert(acct.id.clone(), engine);

                        let watcher = cx.new({
                            let id = acct.id.clone();
                            let p = pool.clone();
                            move |cx| IdleWatcher::new(id, p, "INBOX".into(), cx)
                        });
                        this._subscriptions
                            .push(cx.subscribe(&watcher, Self::on_idle_event));
                        this.idle_watchers.insert(acct.id.clone(), watcher);
                    }

                    // Create SearchEngine if semantic search is enabled
                    let app_config = config::load().unwrap_or_default();
                    if app_config.semantic_search.enabled {
                        let engine = cx.new({
                            let p = pool.clone();
                            let model = app_config.semantic_search.model.clone();
                            move |cx| SearchEngine::new(p, model, true, cx)
                        });
                        this._subscriptions
                            .push(cx.subscribe(&engine, Self::on_search_engine_event));
                        this.search_engine = Some(engine);
                    }

                    cx.notify();
                })
                .ok();
            }));
        }

        view
    }

    fn on_sync_event(
        &mut self,
        engine: Entity<SyncEngine>,
        _event: &SyncEvent,
        cx: &mut Context<Self>,
    ) {
        // Determine which account this engine belongs to
        let engine_account_id = self
            .sync_engines
            .iter()
            .find(|(_, e)| **e == engine)
            .map(|(id, _)| id.clone());

        let Some(engine_account_id) = engine_account_id else {
            return;
        };

        // Only reload UI if this event is for the currently active account
        if engine_account_id != self.active_account_id {
            return;
        }

        let status = engine.read(cx).status().clone();

        let should_reload = matches!(
            status,
            SyncStatus::MailboxesSynced
                | SyncStatus::SyncingMessages { .. }
                | SyncStatus::Completed { .. }
        );

        if should_reload {
            self.reload_from_db(cx);
        }

        // Trigger incremental embedding after sync completes
        if matches!(status, SyncStatus::Completed { .. }) {
            if let Some(engine) = &self.search_engine {
                engine.update(cx, |e, cx| e.embed_new_messages(cx));
            }
        }

        cx.notify();
    }

    fn on_idle_event(
        &mut self,
        _watcher: Entity<IdleWatcher>,
        event: &IdleEvent,
        cx: &mut Context<Self>,
    ) {
        if event.account_id != self.active_account_id {
            return;
        }
        // Suppress IDLE reloads while local actions are in-flight to avoid
        // overwriting optimistic state with stale server data.
        if self.action_handler.read(cx).has_pending_actions() {
            return;
        }
        let active_folder = self
            .mailbox
            .read(cx)
            .active_folder()
            .map(|f| f.name.clone());
        if active_folder.as_deref() == Some(&event.mailbox) {
            self.reload_from_db(cx);

            // Trigger incremental embedding for new messages
            if let Some(engine) = &self.search_engine {
                engine.update(cx, |e, cx| e.embed_new_messages(cx));
            }
        }
    }

    fn on_actions_settled(
        &mut self,
        _handler: Entity<MailActionHandler>,
        _event: &ActionsSettled,
        cx: &mut Context<Self>,
    ) {
        // All IMAP actions completed — reload from DB to reconcile with server state.
        self.reload_from_db(cx);
    }

    fn on_search_engine_event(
        &mut self,
        _engine: Entity<SearchEngine>,
        _event: &SearchEngineEvent,
        cx: &mut Context<Self>,
    ) {
        cx.notify();
    }

    fn on_search_input_event(
        &mut self,
        _input: Entity<InputState>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event, InputEvent::Change) {
            return;
        }

        let query = self.search_input.read(cx).value().to_string();

        if query.is_empty() {
            // Clear search, restore previous folder
            self.search_query.clear();
            self.search_results.clear();
            self.is_searching = false;
            self._search_debounce = None;

            if let Some(folder_idx) = self.pre_search_folder.take() {
                self.select_folder(folder_idx, cx);
            }
            cx.notify();
            return;
        }

        // Save current folder before first search
        if self.search_query.is_empty() {
            self.pre_search_folder = Some(self.mailbox.read(cx).active_folder);
        }

        self.search_query = query.clone();

        // Reset engine status if a previous search was in-flight (its task is about to be dropped)
        if let Some(engine) = &self.search_engine {
            engine.update(cx, |e, cx| {
                if matches!(e.status(), SearchEngineStatus::Searching) {
                    e.force_ready(cx);
                }
            });
        }

        // Debounce 250ms
        self._search_debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(250))
                .await;

            let search_task = this
                .update(cx, |this, cx| {
                    let Some(engine) = &this.search_engine else {
                        return None;
                    };
                    this.is_searching = true;
                    cx.notify();
                    Some(engine.update(cx, |e, cx| e.search(query.clone(), cx)))
                })
                .ok()
                .flatten();

            let Some(task) = search_task else {
                return;
            };

            let results = task.await;

            let _ = this.update(cx, |this, cx| {
                this.search_results = results.clone();
                this.is_searching = false;
                this.search_list.update(cx, |list, cx| {
                    list.delegate_mut().results = results;
                    list.delegate_mut().searching = false;
                    cx.notify();
                });
                cx.notify();
            });
        }));
    }

    fn reload_from_db(&mut self, cx: &mut Context<Self>) {
        let active_name = self
            .mailbox
            .read(cx)
            .active_folder()
            .map(|f| f.name.clone());
        self.reload_from_db_with_folder(active_name, cx);
    }

    fn reload_from_db_with_folder(&mut self, folder: Option<String>, cx: &mut Context<Self>) {
        let mailbox = self.mailbox.clone();
        let Some(pool) = self.mailbox.read(cx).pool().cloned() else {
            return;
        };
        let account_id = self.mailbox.read(cx).account_id.clone();
        let restore_nav = folder.is_some();

        cx.spawn(async move |this, cx| {
            let loaded =
                Tokio::spawn(cx, {
                    let pool = pool.clone();
                    let account_id = account_id.clone();
                    async move {
                        MailboxState::load_from_db(&pool, &account_id, folder.as_deref()).await
                    }
                })
                .await;

            let _ = mailbox.update(cx, |state, cx| {
                if let Ok(data) = loaded {
                    state.apply_loaded_data(data);
                }
                cx.notify();
            });

            if restore_nav {
                let _ = this.update(cx, |this, cx| {
                    this.restore_nav_state(cx);
                });
            }
        })
        .detach();
    }

    fn save_nav_state(&mut self, cx: &mut Context<Self>) {
        let Some(folder_name) = self
            .mailbox
            .read(cx)
            .active_folder()
            .map(|f| f.name.clone())
        else {
            return;
        };
        let selected = self.mail_list.read(cx).delegate().selected_index;
        let offset = self
            .mail_list
            .read(cx)
            .scroll_handle()
            .base_handle()
            .offset();

        let key = (self.active_account_id.clone(), folder_name.clone());
        self.nav_cache.insert(
            key,
            SavedNavState {
                selected_index: selected,
                scroll_offset: offset,
            },
        );

        // Persist to DB
        if let Some(pool) = self.pool.clone() {
            let db_key = format!("nav:{}:{}", self.active_account_id, folder_name);
            let selected_row = selected.map(|ip| ip.row);
            let value = serde_json::json!({
                "selected_row": selected_row,
                "scroll_x": f32::from(offset.x),
                "scroll_y": f32::from(offset.y),
            })
            .to_string();
            cx.spawn(async move |_this, cx| {
                let _ = Tokio::spawn(
                    cx,
                    async move { ui_state::set(&pool, &db_key, &value).await },
                )
                .await;
            })
            .detach();
        }
    }

    fn restore_nav_state(&mut self, cx: &mut Context<Self>) {
        let Some(folder_name) = self
            .mailbox
            .read(cx)
            .active_folder()
            .map(|f| f.name.clone())
        else {
            return;
        };
        let key = (self.active_account_id.clone(), folder_name.clone());

        // Try in-memory cache first
        if let Some(saved) = self.nav_cache.get(&key) {
            let selected = saved.selected_index;
            let offset = saved.scroll_offset;
            self.mail_list.update(cx, |list, cx| {
                list.delegate_mut().selected_index = selected;
                list.scroll_handle().base_handle().set_offset(offset);
                cx.notify();
            });
            return;
        }

        // Fall back to DB
        let Some(pool) = self.pool.clone() else {
            return;
        };
        let db_key = format!("nav:{}:{}", self.active_account_id, folder_name);
        let mail_list = self.mail_list.clone();

        cx.spawn(async move |_this, cx| {
            let value = Tokio::spawn(cx, async move { ui_state::get(&pool, &db_key).await }).await;

            let Some(json_str) = value.ok().and_then(|r| r.ok()).flatten() else {
                return;
            };
            let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json_str) else {
                return;
            };

            let selected = parsed
                .get("selected_row")
                .and_then(|v| v.as_u64())
                .map(|row| gpui_component::IndexPath::new(row as usize));
            let scroll_x = parsed
                .get("scroll_x")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32;
            let scroll_y = parsed
                .get("scroll_y")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32;
            let offset = Point::new(px(scroll_x), px(scroll_y));

            let _ = mail_list.update(cx, |list, cx| {
                list.delegate_mut().selected_index = selected;
                list.scroll_handle().base_handle().set_offset(offset);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn switch_account(&mut self, account_id: &str, cx: &mut Context<Self>) {
        if account_id == self.active_account_id {
            return;
        }

        let Some(acct) = self.accounts.iter().find(|a| a.id == account_id).cloned() else {
            return;
        };

        // Save current account/folder's navigation state
        self.save_nav_state(cx);

        // Remember which folder was active for the current account
        let current_folder = self
            .mailbox
            .read(cx)
            .active_folder()
            .map(|f| f.name.clone());
        if let Some(folder) = current_folder {
            self.last_folder
                .insert(self.active_account_id.clone(), folder);
        }

        self.active_account_id = acct.id.clone();

        // Persist active account selection to DB
        if let Some(pool) = self.pool.clone() {
            let id = acct.id.clone();
            cx.spawn(async move |_this, cx| {
                let _ = Tokio::spawn(cx, async move {
                    ui_state::set(&pool, "active_account", &id).await
                })
                .await;
            })
            .detach();
        }

        // Restore the target account's active folder (if we visited it before)
        let saved_folder = self.last_folder.get(&acct.id).cloned();

        self.mailbox.update(cx, |state, _| {
            state.switch_to(
                acct.id.clone(),
                acct.display_name.clone(),
                acct.email.clone(),
            );
        });

        self.mail_list.update(cx, |list, cx| {
            list.delegate_mut().selected_index = None;
            cx.notify();
        });

        self.reload_from_db_with_folder(saved_folder, cx);
        cx.notify();
    }

    pub fn add_account(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wizard = cx.new(|cx| SetupWizard::new(window, cx));
        self._subscriptions
            .push(cx.subscribe_in(&wizard, window, Self::on_new_account_saved));

        let wizard_for_dialog = wizard.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            let wizard = wizard_for_dialog.clone();
            dialog
                .w(px(520.))
                .overlay(true)
                .overlay_closable(true)
                .p_0()
                .content(move |content, _, _| content.child(wizard.clone()))
        });
    }

    fn on_new_account_saved(
        &mut self,
        _wizard: &Entity<SetupWizard>,
        _event: &AccountSaved,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.close_dialog(cx);

        // Reload config to pick up the new account
        let app_config = config::load().unwrap_or_default();
        let new_accounts: Vec<_> = app_config
            .accounts
            .iter()
            .filter(|a| !self.accounts.iter().any(|existing| existing.id == a.id))
            .cloned()
            .collect();

        self.accounts = app_config.accounts;

        // Start sync engines for new accounts
        for acct in &new_accounts {
            if let Some(pool) = &self.pool {
                let engine = cx.new({
                    let id = acct.id.clone();
                    let p = pool.clone();
                    let interval = acct.sync_interval_secs;
                    move |cx| SyncEngine::new(id, p, interval, cx)
                });
                self._subscriptions
                    .push(cx.subscribe(&engine, Self::on_sync_event));
                self.sync_engines.insert(acct.id.clone(), engine);

                let watcher = cx.new({
                    let id = acct.id.clone();
                    let p = pool.clone();
                    move |cx| IdleWatcher::new(id, p, "INBOX".into(), cx)
                });
                self._subscriptions
                    .push(cx.subscribe(&watcher, Self::on_idle_event));
                self.idle_watchers.insert(acct.id.clone(), watcher);
            }
        }

        // Switch to the first new account
        if let Some(acct) = new_accounts.first() {
            self.switch_account(&acct.id.clone(), cx);
        }
    }

    pub fn remove_account(&mut self, account_id: &str, cx: &mut Context<Self>) {
        let account_id = account_id.to_string();

        // Remove sync engine and idle watcher (drops them, stopping sync + IDLE)
        self.sync_engines.remove(&account_id);
        self.idle_watchers.remove(&account_id);
        self.accounts.retain(|a| a.id != account_id);

        // Async cleanup: config, credentials, DB data
        let pool = self.pool.clone();
        cx.spawn({
            let account_id = account_id.clone();
            async move |_this, cx| {
                let result = Tokio::spawn(cx, {
                    let account_id = account_id.clone();
                    async move {
                        let mut mgr = crate::account::AccountManager::load()?;
                        mgr.remove_account(&account_id).await?;
                        if let Some(pool) = pool {
                            mailboxes::delete_by_account(&pool, &account_id).await?;
                            messages::delete_by_account(&pool, &account_id).await?;
                        }
                        Ok::<(), crate::error::Error>(())
                    }
                })
                .await;

                if let Ok(Err(e)) | Err(e) =
                    result.map_err(|e| crate::error::Error::Config(e.to_string()))
                {
                    tracing::error!(error = %e, "failed to clean up removed account");
                }
            }
        })
        .detach();

        if self.accounts.is_empty() {
            cx.emit(NoAccountsRemaining);
            return;
        }

        // Switch to the first remaining account if we removed the active one
        if self.active_account_id == account_id {
            let next_id = self.accounts[0].id.clone();
            self.switch_account(&next_id, cx);
        }

        cx.notify();
    }

    pub fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.new(|_| SettingsView::new(self.accounts.clone(), self.pool.clone()));
        self._subscriptions
            .push(cx.subscribe_in(&view, window, Self::on_settings_event));
        SettingsView::open_dialog(view, window, cx);
    }

    fn on_settings_event(
        &mut self,
        _view: &Entity<SettingsView>,
        event: &SettingsEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SettingsEvent::Closed => {
                // Reload config to pick up any account field edits or theme changes.
                if let Ok(config) = config::load() {
                    self.accounts = config.accounts.clone();

                    // Check if semantic search settings changed
                    let now_enabled = config.semantic_search.enabled;
                    let was_enabled = self
                        .search_engine
                        .as_ref()
                        .map(|e| e.read(cx).is_enabled())
                        .unwrap_or(false);

                    if now_enabled && !was_enabled {
                        if let Some(pool) = &self.pool {
                            let engine = cx.new({
                                let p = pool.clone();
                                let model = config.semantic_search.model.clone();
                                move |cx| SearchEngine::new(p, model, true, cx)
                            });
                            self._subscriptions
                                .push(cx.subscribe(&engine, Self::on_search_engine_event));
                            self.search_engine = Some(engine);
                        }
                    } else if !now_enabled && was_enabled {
                        if let Some(engine) = &self.search_engine {
                            engine.update(cx, |e, cx| {
                                e.set_enabled(false, String::new(), cx);
                            });
                        }
                    }
                }
                cx.notify();
            }
            SettingsEvent::AccountDeleted(id) => {
                self.remove_account(id, cx);
            }
            SettingsEvent::EmbeddingsReset => {
                if let Some(engine) = &self.search_engine {
                    engine.update(cx, |e, cx| e.embed_new_messages(cx));
                }
            }
            SettingsEvent::ModelChanged(new_model) => {
                if let (Some(pool), Some(engine)) = (&self.pool, &self.search_engine) {
                    let old_model = engine.read(cx).model_name().to_string();
                    if old_model != *new_model {
                        // Delete old model's embeddings and recreate engine with new model
                        let pool_clone = pool.clone();
                        let old = old_model.clone();
                        Tokio::spawn(cx, async move {
                            let _ = crate::search::store::EmbeddingStore::delete_by_model(
                                &pool_clone,
                                &old,
                            )
                            .await;
                        })
                        .detach();

                        let new_engine = cx.new({
                            let p = pool.clone();
                            let model = new_model.clone();
                            move |cx| SearchEngine::new(p, model, true, cx)
                        });
                        self._subscriptions
                            .push(cx.subscribe(&new_engine, Self::on_search_engine_event));
                        self.search_engine = Some(new_engine);
                    }
                }
            }
            SettingsEvent::AccountReset(id) => {
                // Trigger a full re-sync on the account's engine.
                if let Some(engine) = self.sync_engines.get(id) {
                    engine.update(cx, |e, cx| e.trigger_sync(cx));
                }
                tracing::info!(account_id = %id, "account DB reset, re-sync triggered");
            }
        }
    }

    pub fn open_compose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let account_id = self.active_account_id.clone();
        let from_email = self.mailbox.read(cx).account_email.clone();
        let pool = self.pool.clone();

        let compose = cx.new(|cx| ComposeView::new(account_id, from_email, pool, window, cx));
        self.open_compose_dialog(compose, window, cx);
    }

    pub fn open_draft(&mut self, draft: &MailMessage, window: &mut Window, cx: &mut Context<Self>) {
        let pool = self.pool.clone();

        let compose = cx.new(|cx| ComposeView::from_draft(draft, pool, window, cx));
        self.open_compose_dialog(compose, window, cx);
    }

    fn open_compose_dialog(
        &mut self,
        compose: Entity<ComposeView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self._subscriptions
            .push(cx.subscribe_in(&compose, window, Self::on_compose_event));

        let compose_for_dialog = compose.clone();
        let compose_for_close = compose.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            let compose = compose_for_dialog.clone();
            let compose_for_close = compose_for_close.clone();
            dialog
                .w(px(680.))
                .h(px(520.))
                .overlay(true)
                .overlay_closable(true)
                .p_0()
                .on_close(move |_, window, cx| {
                    compose_for_close.update(cx, |view, cx| {
                        view.save_draft(window, cx);
                    });
                })
                .content(move |content, _, _| content.child(compose.clone()))
        });
    }

    fn on_compose_event(
        &mut self,
        _compose: &Entity<ComposeView>,
        event: &ComposeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ComposeEvent::Sent | ComposeEvent::DraftSaved | ComposeEvent::Discarded => {
                window.close_dialog(cx);
            }
        }
        if matches!(event, ComposeEvent::Sent | ComposeEvent::DraftSaved) {
            self.reload_from_db(cx);
        }
    }

    pub fn select_folder(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(pool) = self.mailbox.read(cx).pool().cloned() else {
            return;
        };
        let account_id = self.mailbox.read(cx).account_id.clone();

        // Save scroll/selection state for the folder we're leaving
        self.save_nav_state(cx);

        self.mailbox.update(cx, |state, _| {
            state.select_folder(index);
        });

        self.mail_list.update(cx, |list, cx| {
            list.delegate_mut().selected_index = None;
            cx.notify();
        });

        let mailbox = self.mailbox.clone();
        let mailbox_name = self
            .mailbox
            .read(cx)
            .active_folder()
            .map(|f| f.name.clone())
            .unwrap_or_default();

        // Switch IDLE watcher to the new folder
        if let Some(watcher) = self.idle_watchers.get(&self.active_account_id) {
            watcher.update(cx, |w, _| {
                w.watch_mailbox(mailbox_name.clone());
            });
        }

        cx.spawn(async move |this, cx| {
            let messages = Tokio::spawn(cx, async move {
                MailboxState::refresh_active_folder(&pool, &account_id, &mailbox_name).await
            })
            .await;

            let _ = mailbox.update(cx, |state, cx| {
                if let Ok(msgs) = messages {
                    state.set_messages(msgs);
                }
                cx.notify();
            });

            // Restore scroll/selection for the newly selected folder
            let _ = this.update(cx, |this, cx| {
                this.restore_nav_state(cx);
            });
        })
        .detach();
    }
}

impl Render for InboxView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_engine = self.sync_engines.get(&self.active_account_id);
        let (sync_text, sync_progress) = active_engine
            .map(|e| {
                let s = e.read(cx).status();
                (s.display(), s.progress())
            })
            .unwrap_or(("Initializing...".into(), None));

        let semantic_enabled = self
            .search_engine
            .as_ref()
            .map(|e| e.read(cx).is_enabled())
            .unwrap_or(false);

        let (embed_text, embed_progress) = self
            .search_engine
            .as_ref()
            .map(|e| {
                let s = e.read(cx).status();
                (s.display(), s.progress())
            })
            .unwrap_or((None, None));

        let in_search = semantic_enabled && !self.search_query.is_empty();

        let sidebar = sidebar::render_sidebar(
            &self.mailbox,
            &self.sync_engines,
            &self.accounts,
            &self.active_account_id,
            self.collapsed,
            window,
            cx,
        );

        let content_list: AnyElement = if in_search {
            List::new(&self.search_list)
                .flex_1()
                .w_full()
                .px_2()
                .pt_1()
                .into_any_element()
        } else {
            List::new(&self.mail_list)
                .flex_1()
                .w_full()
                .px_2()
                .pt_1()
                .into_any_element()
        };

        h_flex().size_full().child(sidebar).child(
            v_flex()
                .flex_1()
                .size_full()
                .child(
                    div()
                        .relative()
                        .w_full()
                        .child(
                            h_flex()
                                .items_start()
                                .gap_2()
                                .p_2()
                                .overflow_hidden()
                                .child(
                                    SidebarToggleButton::new()
                                        .side(Side::Left)
                                        .collapsed(self.collapsed)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.collapsed = !this.collapsed;
                                            cx.notify();
                                        })),
                                )
                                .child({
                                    let mb = self.mailbox.read(cx);
                                    let theme = cx.theme();
                                    let segments = mb
                                        .active_folder()
                                        .map(|f| f.breadcrumb_segments())
                                        .unwrap_or_else(|| vec!["Loading...".into()]);

                                    h_flex()
                                        .items_center()
                                        .gap_1()
                                        .text_sm()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .flex_shrink()
                                        .children(segments.iter().enumerate().flat_map(
                                            |(i, segment)| {
                                                let mut els: Vec<AnyElement> = Vec::new();
                                                if i > 0 {
                                                    els.push(
                                                        gpui_component::Icon::new(
                                                            IconName::ChevronRight,
                                                        )
                                                        .size_3()
                                                        .text_color(theme.muted_foreground)
                                                        .into_any_element(),
                                                    );
                                                }
                                                let weight = if i == segments.len() - 1 {
                                                    FontWeight::SEMIBOLD
                                                } else {
                                                    FontWeight::NORMAL
                                                };
                                                els.push(
                                                    div()
                                                        .font_weight(weight)
                                                        .when(i < segments.len() - 1, |el| {
                                                            el.text_color(theme.muted_foreground)
                                                        })
                                                        .child(segment.clone())
                                                        .into_any_element(),
                                                );
                                                els
                                            },
                                        ))
                                }),
                        )
                        .when(semantic_enabled, |el| {
                            el.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .w_full()
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        div().w(px(360.)).child(
                                            Input::new(&self.search_input)
                                                .prefix(
                                                    gpui_component::Icon::new(IconName::Search)
                                                        .size_3p5()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .into_any_element(),
                                                )
                                                .cleanable(true)
                                                .small(),
                                        ),
                                    ),
                            )
                        }),
                )
                .child(div().border_t_1().border_color(cx.theme().border))
                .child(content_list)
                .child({
                    let mb = self.mailbox.read(cx);
                    let total = mb.messages.len();
                    let unread = mb.messages.iter().filter(|m| !m.is_read).count();
                    let on_settings = cx.listener(|this, _ev: &ClickEvent, window, cx| {
                        this.open_settings(window, cx);
                    });
                    StatusBar::new()
                        .mail_count(total)
                        .unread_count(unread)
                        .sync_status(sync_text)
                        .sync_progress(sync_progress)
                        .embed_status(embed_text)
                        .embed_progress(embed_progress)
                        .on_settings(on_settings)
                }),
        )
    }
}
