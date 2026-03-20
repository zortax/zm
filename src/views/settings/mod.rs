mod accounts;
mod general;

use gpui::*;
use gpui_component::{
    Sizable, Size, WindowExt as _, group_box::GroupBoxVariant, setting::Settings,
};
use gpui_tokio::Tokio;

use crate::config::AccountConfig;
use crate::db::repo::{mailboxes, messages};

pub enum SettingsEvent {
    /// Emitted when the dialog closes so InboxView can reload config.
    Closed,
    AccountDeleted(String),
    AccountReset(String),
    EmbeddingsReset,
    ModelChanged(String),
}

impl EventEmitter<SettingsEvent> for SettingsView {}

pub struct SettingsView {
    accounts: Vec<AccountConfig>,
    pool: Option<sqlx::SqlitePool>,
    _task: Option<Task<()>>,
}

impl SettingsView {
    pub fn new(accounts: Vec<AccountConfig>, pool: Option<sqlx::SqlitePool>) -> Self {
        Self {
            accounts,
            pool,
            _task: None,
        }
    }

    pub fn open_dialog(view: Entity<Self>, window: &mut Window, cx: &mut App) {
        let view_for_close = view.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            let view_for_close = view_for_close.clone();
            dialog
                .w(px(800.))
                .h(px(560.))
                .close_button(false)
                .overlay(true)
                .overlay_closable(true)
                .p_0()
                .on_close(move |_, _, cx| {
                    view_for_close.update(cx, |_, cx| {
                        cx.emit(SettingsEvent::Closed);
                    });
                })
                .content(move |content, _, _| content.child(view.clone()))
        });
    }

    pub fn reset_account_db(&mut self, account_id: String, cx: &mut Context<Self>) {
        let pool = self.pool.clone();
        self._task = Some(cx.spawn({
            let account_id = account_id.clone();
            async move |this, cx| {
                if let Some(pool) = pool {
                    let id = account_id.clone();
                    let result = Tokio::spawn(cx, async move {
                        mailboxes::delete_by_account(&pool, &id).await?;
                        messages::delete_by_account(&pool, &id).await?;
                        Ok::<(), crate::error::Error>(())
                    })
                    .await;

                    if let Ok(Err(e)) | Err(e) =
                        result.map_err(|e| crate::error::Error::Config(e.to_string()))
                    {
                        tracing::error!(error = %e, "failed to reset account DB");
                    }
                }

                let _ = this.update(cx, |_, cx| {
                    cx.emit(SettingsEvent::AccountReset(account_id));
                });
            }
        }));
    }

    pub fn reset_embeddings(&mut self, cx: &mut Context<Self>) {
        let pool = self.pool.clone();
        self._task = Some(cx.spawn(async move |this, cx| {
            if let Some(pool) = pool {
                let result = Tokio::spawn(cx, async move {
                    sqlx::query("DELETE FROM embeddings")
                        .execute(&pool)
                        .await
                        .map_err(|e| crate::error::Error::Config(e.to_string()))?;
                    Ok::<(), crate::error::Error>(())
                })
                .await;

                if let Ok(Err(e)) | Err(e) =
                    result.map_err(|e| crate::error::Error::Config(e.to_string()))
                {
                    tracing::error!(error = %e, "failed to reset embeddings");
                }
            }

            let _ = this.update(cx, |_, cx| {
                cx.emit(SettingsEvent::EmbeddingsReset);
            });
        }));
    }

    pub fn delete_account(&mut self, account_id: String, cx: &mut Context<Self>) {
        self.accounts.retain(|a| a.id != account_id);
        cx.emit(SettingsEvent::AccountDeleted(account_id));
        cx.notify();
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().clone();
        let general_page = general::build_page(&view, cx);
        let accounts_page = accounts::build_page(&view, &self.accounts);

        Settings::new("zm-settings")
            .with_size(Size::Small)
            .with_group_variant(GroupBoxVariant::Outline)
            .sidebar_width(px(200.))
            .pages(vec![general_page, accounts_page])
    }
}
