use gpui::*;
use gpui_component::input::InputState;
use gpui_component::v_flex;

use crate::assets::IconName;
use crate::components::readonly_field::ReadonlyField;

/// Displays mail metadata (From, To, Date) as a vertical list of read-only fields.
#[derive(IntoElement)]
pub struct MailMetadata {
    from_input: Entity<InputState>,
    to_input: Entity<InputState>,
    date_input: Entity<InputState>,
}

impl MailMetadata {
    pub fn new(
        from_input: &Entity<InputState>,
        to_input: &Entity<InputState>,
        date_input: &Entity<InputState>,
    ) -> Self {
        Self {
            from_input: from_input.clone(),
            to_input: to_input.clone(),
            date_input: date_input.clone(),
        }
    }
}

impl RenderOnce for MailMetadata {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        v_flex()
            .gap_2()
            .text_sm()
            .child(ReadonlyField::new(IconName::User, "From", &self.from_input))
            .child(ReadonlyField::new(IconName::Users, "To", &self.to_input))
            .child(ReadonlyField::new(
                IconName::Calendar,
                "Date",
                &self.date_input,
            ))
    }
}
