use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{ActiveTheme, Icon, Selectable, h_flex};

use crate::assets::IconName;
use crate::search::store::SearchResult;

#[derive(IntoElement)]
pub struct SearchResultItem {
    id: ElementId,
    from_name: SharedString,
    subject: SharedString,
    preview: SharedString,
    date: SharedString,
    folder: SharedString,
    is_read: bool,
    is_starred: bool,
    selected: bool,
}

impl SearchResultItem {
    pub fn new(id: impl Into<ElementId>, result: &SearchResult, selected: bool) -> Self {
        let preview_text = result
            .body
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(80)
            .collect::<String>();
        Self {
            id: id.into(),
            from_name: result.from_name.clone().into(),
            subject: result.subject.clone().into(),
            preview: format!(" — {preview_text}").into(),
            date: crate::db::repo::messages::format_date_public(&result.date).into(),
            folder: result.mailbox_name.clone().into(),
            is_read: result.is_read,
            is_starred: result.is_starred,
            selected,
        }
    }
}

impl Selectable for SearchResultItem {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for SearchResultItem {
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

        h_flex()
            .id(self.id)
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
            // Star + Sender
            .child(
                h_flex()
                    .gap_2()
                    .flex_shrink_0()
                    .w(px(150.))
                    .items_center()
                    .overflow_x_hidden()
                    .child(star_icon.size_4())
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
            // Folder badge
            .child(
                div()
                    .flex_shrink_0()
                    .px_1p5()
                    .py_0p5()
                    .rounded_sm()
                    .bg(theme.muted)
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .whitespace_nowrap()
                    .child(self.folder),
            )
            // Subject + preview
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
            // Date
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .whitespace_nowrap()
                    .text_color(if is_unread {
                        theme.foreground
                    } else {
                        theme.muted_foreground
                    })
                    .font_weight(font_weight)
                    .child(self.date),
            )
    }
}
