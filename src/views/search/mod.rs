pub mod search_result_item;

use gpui::*;
use gpui_component::IndexPath;
use gpui_component::list::{ListDelegate, ListState};

use crate::search::store::SearchResult;
use crate::views::mail_detail::open_mail_dialog;
use crate::views::search::search_result_item::SearchResultItem;

pub struct SearchListDelegate {
    pub results: Vec<SearchResult>,
    pub selected_index: Option<IndexPath>,
    pub searching: bool,
}

impl SearchListDelegate {
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
            selected_index: None,
            searching: false,
        }
    }
}

impl ListDelegate for SearchListDelegate {
    type Item = SearchResultItem;

    fn items_count(&self, _section: usize, _cx: &App) -> usize {
        self.results.len()
    }

    fn perform_search(
        &mut self,
        _query: &str,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) -> Task<()> {
        // Search is handled externally by SearchEngine
        Task::ready(())
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        self.selected_index = ix;
        cx.notify();
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        let result = self.results.get(ix.row)?;
        Some(SearchResultItem::new(ix, result, false))
    }

    fn confirm(
        &mut self,
        _secondary: bool,
        window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        if let Some(ix) = self.selected_index {
            if let Some(result) = self.results.get(ix.row) {
                // Convert SearchResult to MailMessage for the detail dialog
                let msg = crate::state::mail::MailMessage {
                    id: result.message_id,
                    account_id: result.account_id.clone(),
                    mailbox_name: result.mailbox_name.clone(),
                    uid: result.uid,
                    from_name: result.from_name.clone(),
                    from_email: result.from_email.clone(),
                    to: serde_json::from_str(&result.to_addresses).unwrap_or_default(),
                    subject: result.subject.clone(),
                    date: crate::db::repo::messages::format_date_public(&result.date),
                    body: result.body.clone(),
                    is_read: result.is_read,
                    is_starred: result.is_starred,
                };
                cx.spawn_in(window, async move |_view, window| {
                    _ = window.update(|window, cx| {
                        open_mail_dialog(&msg, window, cx);
                    });
                })
                .detach();
            }
        }
    }

    fn loading(&self, _cx: &App) -> bool {
        self.searching
    }
}
