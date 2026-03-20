use gpui::*;
use gpui_component::{
    Disableable, Icon, Sizable,
    button::{Button, ButtonVariants as _},
    h_flex,
};

use crate::assets::IconName;

/// Footer toolbar with Send, Save Draft, and Discard buttons.
#[derive(IntoElement)]
pub struct ComposeActions {
    on_send: Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
    on_save_draft: Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
    on_discard: Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
    sending: bool,
}

impl ComposeActions {
    pub fn new(
        on_send: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        on_save_draft: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        on_discard: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        sending: bool,
    ) -> Self {
        Self {
            on_send: Box::new(on_send),
            on_save_draft: Box::new(on_save_draft),
            on_discard: Box::new(on_discard),
            sending,
        }
    }
}

impl RenderOnce for ComposeActions {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        h_flex()
            .w_full()
            .justify_end()
            .gap_2()
            .child(
                Button::new("discard")
                    .ghost()
                    .compact()
                    .small()
                    .icon(Icon::new(IconName::Trash2))
                    .label("Discard")
                    .on_click(self.on_discard),
            )
            .child(
                Button::new("save-draft")
                    .ghost()
                    .compact()
                    .small()
                    .icon(Icon::new(IconName::FileText))
                    .label("Save Draft")
                    .on_click(self.on_save_draft),
            )
            .child(
                Button::new("send")
                    .primary()
                    .compact()
                    .small()
                    .icon(Icon::new(IconName::Send))
                    .label("Send")
                    .loading(self.sending)
                    .disabled(self.sending)
                    .on_click(self.on_send),
            )
    }
}
