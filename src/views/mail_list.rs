use std::rc::Rc;

use gpui::*;
use gpui_component::IndexPath;
use gpui_component::list::{ListDelegate, ListState};

use crate::actions::MailActionHandler;
use crate::components::mail_list_item::MailListItem;
use crate::state::mail::{FolderKind, MailMessage};
use crate::state::mailbox::MailboxState;
use crate::views::mail_detail::open_mail_dialog;

pub struct MailListDelegate {
    pub mailbox: Entity<MailboxState>,
    pub action_handler: Entity<MailActionHandler>,
    pub selected_index: Option<IndexPath>,
    pub on_open_draft: Option<Rc<dyn Fn(MailMessage, &mut Window, &mut App)>>,
    filtered_indices: Vec<usize>,
    has_filter: bool,
}

impl MailListDelegate {
    pub fn new(mailbox: Entity<MailboxState>, action_handler: Entity<MailActionHandler>) -> Self {
        Self {
            mailbox,
            action_handler,
            selected_index: None,
            on_open_draft: None,
            filtered_indices: Vec::new(),
            has_filter: false,
        }
    }

    fn message_index(&self, row: usize) -> Option<usize> {
        if self.has_filter {
            self.filtered_indices.get(row).copied()
        } else {
            Some(row)
        }
    }

    fn visible_count(&self, cx: &App) -> usize {
        if self.has_filter {
            self.filtered_indices.len()
        } else {
            self.mailbox.read(cx).messages.len()
        }
    }
}

impl ListDelegate for MailListDelegate {
    type Item = MailListItem;

    fn items_count(&self, _section: usize, cx: &App) -> usize {
        self.visible_count(cx)
    }

    fn perform_search(
        &mut self,
        query: &str,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Task<()> {
        let query_lower = query.to_lowercase();
        if query_lower.is_empty() {
            self.has_filter = false;
            self.filtered_indices.clear();
        } else {
            let mailbox = self.mailbox.read(cx);
            self.filtered_indices = mailbox
                .messages
                .iter()
                .enumerate()
                .filter(|(_, msg)| {
                    msg.subject.to_lowercase().contains(&query_lower)
                        || msg.from_name.to_lowercase().contains(&query_lower)
                        || msg.from_email.to_lowercase().contains(&query_lower)
                })
                .map(|(i, _)| i)
                .collect();
            self.has_filter = true;
        }
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
        cx: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        let msg_idx = self.message_index(ix.row)?;
        let mailbox = self.mailbox.read(cx);
        let msg = mailbox.messages.get(msg_idx)?;
        let is_draft = mailbox
            .active_folder()
            .map(|f| f.kind == FolderKind::Drafts)
            .unwrap_or(false);

        let mut item = MailListItem::new(ix, msg, false, self.action_handler.clone(), msg_idx);
        if is_draft {
            let to_display = if msg.to.is_empty() {
                "(no recipient)".to_string()
            } else {
                msg.to.join(", ")
            };
            item = item.display_name(to_display);
        }
        Some(item)
    }

    fn confirm(
        &mut self,
        _secondary: bool,
        window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        if let Some(ix) = self.selected_index {
            let Some(msg_idx) = self.message_index(ix.row) else {
                return;
            };
            self.action_handler.update(cx, |handler, cx| {
                handler.mark_read(msg_idx, cx);
            });

            let mb = self.mailbox.read(cx);
            let is_draft = mb
                .active_folder()
                .map(|f| f.kind == FolderKind::Drafts)
                .unwrap_or(false);
            let msg = mb.messages.get(msg_idx).cloned();

            if let Some(msg) = msg {
                if is_draft {
                    if let Some(on_open_draft) = &self.on_open_draft {
                        on_open_draft(msg, window, cx);
                    }
                } else {
                    cx.spawn_in(window, async move |_view, window| {
                        _ = window.update(|window, cx| {
                            open_mail_dialog(&msg, window, cx);
                        });
                    })
                    .detach();
                }
            }
        }
    }

    fn loading(&self, cx: &App) -> bool {
        self.mailbox.read(cx).loading
    }
}
