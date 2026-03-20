use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{ActiveTheme, Icon, Selectable, h_flex};

use crate::actions::MailActionHandler;
use crate::assets::IconName;
use crate::state::mail::MailMessage;

#[derive(IntoElement)]
pub struct MailListItem {
    id: ElementId,
    action_handler: Entity<MailActionHandler>,
    msg_index: usize,
    from_name: SharedString,
    subject: SharedString,
    preview: SharedString,
    date: SharedString,
    is_read: bool,
    is_starred: bool,
    selected: bool,
}

impl MailListItem {
    pub fn new(
        id: impl Into<ElementId>,
        msg: &MailMessage,
        selected: bool,
        action_handler: Entity<MailActionHandler>,
        msg_index: usize,
    ) -> Self {
        Self {
            id: id.into(),
            action_handler,
            msg_index,
            from_name: msg.from_name.clone().into(),
            subject: msg.subject.clone().into(),
            preview: format!(" — {}", msg.preview()).into(),
            date: msg.date.clone().into(),
            is_read: msg.is_read,
            is_starred: msg.is_starred,
            selected,
        }
    }

    /// Override the sender column to show a different name (e.g. "To" address for drafts).
    pub fn display_name(mut self, name: impl Into<SharedString>) -> Self {
        self.from_name = name.into();
        self
    }
}

impl Selectable for MailListItem {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for MailListItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let is_unread = !self.is_read;

        let text_color = if is_unread {
            theme.foreground
        } else {
            theme.muted_foreground
        };

        let subject_color = if is_unread {
            theme.foreground
        } else {
            theme.foreground.opacity(0.8)
        };

        let font_weight = if is_unread {
            FontWeight::BOLD
        } else {
            FontWeight::NORMAL
        };

        let star_icon = if self.is_starred {
            Icon::new(IconName::Star).text_color(theme.warning)
        } else {
            Icon::new(IconName::StarOff).text_color(theme.muted_foreground.opacity(0.3))
        };

        let bg = if is_unread {
            Some(theme.list_hover.opacity(0.5))
        } else {
            None
        };

        let read_icon = if is_unread {
            IconName::MailOpen
        } else {
            IconName::Mail
        };

        let handler_for_trash = self.action_handler.clone();
        let handler_for_read = self.action_handler.clone();
        let handler_for_star = self.action_handler.clone();
        let msg_index = self.msg_index;

        // Action buttons shown on hover (positioned over the date)
        let actions = h_flex()
            .gap_1()
            .flex_shrink_0()
            .child(
                div()
                    .id("action-trash")
                    .cursor_pointer()
                    .rounded(theme.radius)
                    .p_1()
                    .hover(|s| s.bg(theme.muted))
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        handler_for_trash.update(cx, |handler, cx| {
                            handler.delete(msg_index, cx);
                        });
                    })
                    .child(
                        Icon::new(IconName::Trash2)
                            .size_4()
                            .text_color(theme.muted_foreground),
                    ),
            )
            .child(
                div()
                    .id("action-read-toggle")
                    .cursor_pointer()
                    .rounded(theme.radius)
                    .p_1()
                    .hover(|s| s.bg(theme.muted))
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        handler_for_read.update(cx, |handler, cx| {
                            handler.toggle_read(msg_index, cx);
                        });
                    })
                    .child(
                        Icon::new(read_icon)
                            .size_4()
                            .text_color(theme.muted_foreground),
                    ),
            );

        h_flex()
            .id(self.id)
            .group("")
            .px_4()
            .py_1p5()
            .rounded(theme.radius)
            .overflow_x_hidden()
            .items_center()
            .gap_3()
            .w_full()
            .text_sm()
            .cursor_pointer()
            .when_some(bg, |el, bg| el.bg(bg))
            .hover(|s| s.bg(theme.list_hover))
            // Star + Sender (fixed width left column)
            .child(
                h_flex()
                    .gap_2()
                    .flex_shrink_0()
                    .w(px(180.))
                    .items_center()
                    .overflow_x_hidden()
                    .child(
                        div()
                            .id("action-star")
                            .cursor_pointer()
                            .flex_shrink_0()
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                handler_for_star.update(cx, |handler, cx| {
                                    handler.toggle_star(msg_index, cx);
                                });
                            })
                            .child(star_icon.size_4()),
                    )
                    .child(
                        div()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .overflow_x_hidden()
                            .font_weight(font_weight)
                            .text_color(text_color)
                            .child(self.from_name),
                    ),
            )
            // Subject + preview (flexible middle)
            .child(
                h_flex()
                    .flex_1()
                    .overflow_x_hidden()
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_weight(font_weight)
                            .text_color(subject_color)
                            .whitespace_nowrap()
                            .child(self.subject),
                    )
                    .child(
                        div()
                            .flex_1()
                            .overflow_x_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(theme.muted_foreground)
                            .child(self.preview),
                    ),
            )
            // Date (right side) with hover actions overlay
            .child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .child(
                        div()
                            .text_xs()
                            .whitespace_nowrap()
                            .text_color(if is_unread {
                                theme.foreground
                            } else {
                                theme.muted_foreground
                            })
                            .font_weight(font_weight)
                            .child(self.date)
                            .group_hover("", |s| s.invisible()),
                    )
                    .child(
                        div()
                            .absolute()
                            .right_0()
                            .top(px(-4.))
                            .invisible()
                            .group_hover("", |s| s.visible())
                            .child(actions),
                    ),
            )
    }
}
