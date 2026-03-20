use gpui::*;
use gpui_component::{input::InputState, v_flex};

use crate::assets::IconName;
use crate::components::editable_field::EditableField;
use crate::components::readonly_field::ReadonlyField;

/// Displays compose metadata (From, To, Subject) as a vertical list of fields.
/// From is read-only; To and Subject are editable.
#[derive(IntoElement)]
pub struct ComposeMetadata {
    from_input: Entity<InputState>,
    to_input: Entity<InputState>,
    subject_input: Entity<InputState>,
}

impl ComposeMetadata {
    pub fn new(
        from_input: &Entity<InputState>,
        to_input: &Entity<InputState>,
        subject_input: &Entity<InputState>,
    ) -> Self {
        Self {
            from_input: from_input.clone(),
            to_input: to_input.clone(),
            subject_input: subject_input.clone(),
        }
    }
}

impl RenderOnce for ComposeMetadata {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        v_flex()
            .gap_2()
            .text_sm()
            .child(
                ReadonlyField::new(IconName::User, "From", &self.from_input).label_width(px(56.)),
            )
            .child(EditableField::new(IconName::Users, "To", &self.to_input))
            .child(EditableField::new(
                IconName::Mail,
                "Subject",
                &self.subject_input,
            ))
    }
}
