use gpui::*;
use gpui_component::{ActiveTheme, Icon, h_flex};

use crate::assets::IconName;

/// A single status indicator with an icon and label.
#[derive(IntoElement)]
pub struct StatusItem {
    icon: IconName,
    label: SharedString,
}

impl StatusItem {
    pub fn new(icon: IconName, label: impl Into<SharedString>) -> Self {
        Self {
            icon,
            label: label.into(),
        }
    }
}

impl RenderOnce for StatusItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .gap_1()
            .items_center()
            .child(
                Icon::new(self.icon)
                    .size_3()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.label),
            )
    }
}
