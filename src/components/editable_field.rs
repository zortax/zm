use gpui::*;
use gpui_component::{
    ActiveTheme, Icon, Sizable, h_flex,
    input::{Input, InputState},
};

use crate::assets::IconName;

/// A labeled, editable input field with an icon.
/// Same layout as ReadonlyField but with the input enabled.
#[derive(IntoElement)]
pub struct EditableField {
    icon: IconName,
    label: SharedString,
    input: Entity<InputState>,
    label_width: Pixels,
}

impl EditableField {
    pub fn new(icon: IconName, label: impl Into<SharedString>, input: &Entity<InputState>) -> Self {
        Self {
            icon,
            label: label.into(),
            input: input.clone(),
            label_width: px(56.),
        }
    }
}

impl RenderOnce for EditableField {
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
                div()
                    .flex_1()
                    .child(Input::new(&self.input).appearance(false).small()),
            )
    }
}
