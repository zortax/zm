use gpui::*;
use gpui_component::{
    ActiveTheme, Icon, Sizable, h_flex,
    input::{Input, InputState},
};

use crate::assets::IconName;

/// A labeled, read-only input field with an icon.
/// Text is selectable but not editable.
#[derive(IntoElement)]
pub struct ReadonlyField {
    icon: IconName,
    label: SharedString,
    input: Entity<InputState>,
    label_width: Pixels,
}

impl ReadonlyField {
    pub fn new(icon: IconName, label: impl Into<SharedString>, input: &Entity<InputState>) -> Self {
        Self {
            icon,
            label: label.into(),
            input: input.clone(),
            label_width: px(36.),
        }
    }

    pub fn label_width(mut self, width: Pixels) -> Self {
        self.label_width = width;
        self
    }
}

impl RenderOnce for ReadonlyField {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;

        h_flex()
            .gap_2()
            .items_center()
            .child(
                Icon::new(self.icon)
                    .size_4()
                    .text_color(muted)
                    .flex_shrink_0(),
            )
            .child(
                div()
                    .text_color(muted)
                    .flex_shrink_0()
                    .w(self.label_width)
                    .child(self.label),
            )
            .child(
                div().flex_1().opacity(1.).child(
                    Input::new(&self.input)
                        .appearance(false)
                        .small()
                        .disabled(true),
                ),
            )
    }
}
