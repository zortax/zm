use gpui::*;
use gpui_component::{
    Disableable, Icon, Sizable,
    button::{Button, ButtonVariants as _},
    h_flex,
};

use crate::assets::IconName;

/// A toolbar of mail action buttons (reply, forward, archive, delete).
#[derive(IntoElement)]
pub struct MailActions;

impl MailActions {
    pub fn new() -> Self {
        Self
    }
}

impl RenderOnce for MailActions {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        h_flex()
            .gap_1()
            .child(
                Button::new("reply")
                    .ghost()
                    .small()
                    .icon(Icon::new(IconName::Reply))
                    .tooltip("Reply")
                    .disabled(true),
            )
            .child(
                Button::new("reply-all")
                    .ghost()
                    .small()
                    .icon(Icon::new(IconName::ReplyAll))
                    .tooltip("Reply All")
                    .disabled(true),
            )
            .child(
                Button::new("forward")
                    .ghost()
                    .small()
                    .icon(Icon::new(IconName::Forward))
                    .tooltip("Forward")
                    .disabled(true),
            )
            .child(
                Button::new("archive")
                    .ghost()
                    .small()
                    .icon(Icon::new(IconName::Archive))
                    .tooltip("Archive")
                    .disabled(true),
            )
            .child(
                Button::new("trash")
                    .ghost()
                    .small()
                    .icon(Icon::new(IconName::Trash2))
                    .tooltip("Delete")
                    .disabled(true),
            )
    }
}
