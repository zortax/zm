use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::progress::Progress;
use gpui_component::{ActiveTheme, Icon, h_flex};

use crate::assets::IconName;
use crate::components::status_item::StatusItem;

#[derive(IntoElement)]
pub struct StatusBar {
    items: Vec<StatusItem>,
    sync_text: Option<SharedString>,
    sync_progress: Option<f32>,
    embed_text: Option<SharedString>,
    embed_progress: Option<f32>,
    on_settings: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            sync_text: None,
            sync_progress: None,
            embed_text: None,
            embed_progress: None,
            on_settings: None,
        }
    }

    pub fn item(mut self, item: StatusItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn mail_count(self, count: usize) -> Self {
        self.item(StatusItem::new(
            IconName::Mail,
            format!("{} messages", count),
        ))
    }

    pub fn unread_count(self, count: usize) -> Self {
        self.item(StatusItem::new(
            IconName::MailOpen,
            format!("{} unread", count),
        ))
    }

    pub fn sync_status(mut self, status: impl Into<SharedString>) -> Self {
        self.sync_text = Some(status.into());
        self
    }

    pub fn sync_progress(mut self, progress: Option<f32>) -> Self {
        self.sync_progress = progress;
        self
    }

    pub fn embed_status(mut self, status: Option<impl Into<SharedString>>) -> Self {
        self.embed_text = status.map(|s| s.into());
        self
    }

    pub fn embed_progress(mut self, progress: Option<f32>) -> Self {
        self.embed_progress = progress;
        self
    }

    pub fn on_settings(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_settings = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for StatusBar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        // Sync section: icon | progress bar | text
        let sync_section = h_flex()
            .gap_1()
            .items_center()
            .child(
                Icon::new(IconName::RefreshCw)
                    .size_3()
                    .text_color(theme.muted_foreground),
            )
            .when_some(self.sync_progress, |el, value| {
                el.child(
                    Progress::new("sync-progress")
                        .value(value * 100.0)
                        .w(px(120.)),
                )
            })
            .when_some(self.sync_text, |el, text| {
                el.child(div().text_color(theme.muted_foreground).child(text))
            });

        // Embed section: icon | progress bar | text (only shown when active)
        let embed_section = if self.embed_text.is_some() || self.embed_progress.is_some() {
            Some(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        Icon::new(IconName::Search)
                            .size_3()
                            .text_color(theme.muted_foreground),
                    )
                    .when_some(self.embed_progress, |el, value| {
                        el.child(
                            Progress::new("embed-progress")
                                .value(value * 100.0)
                                .w(px(120.)),
                        )
                    })
                    .when_some(self.embed_text, |el, text| {
                        el.child(div().text_color(theme.muted_foreground).child(text))
                    }),
            )
        } else {
            None
        };

        let muted_fg = theme.muted_foreground;

        h_flex()
            .w_full()
            .px_4()
            .py_1()
            .gap_4()
            .items_center()
            .text_xs()
            .border_t_1()
            .border_color(theme.border)
            .children(self.items)
            .child(sync_section)
            .children(embed_section)
            .child(div().flex_1())
            .when_some(self.on_settings, |el, handler| {
                el.child(
                    div()
                        .id("settings-btn")
                        .cursor_pointer()
                        .rounded_sm()
                        .p_0p5()
                        .hover(|s| s.bg(cx.theme().secondary))
                        .child(
                            Icon::new(IconName::Settings)
                                .size_3p5()
                                .text_color(muted_fg),
                        )
                        .on_click(move |ev, window, cx| handler(ev, window, cx)),
                )
            })
    }
}
