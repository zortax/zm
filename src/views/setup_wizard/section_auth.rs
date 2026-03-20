use gpui::*;
use gpui_component::input::{Input, InputState};
use gpui_component::v_flex;

#[derive(IntoElement)]
pub(crate) struct SectionAuth {
    username_input: Entity<InputState>,
    password_input: Entity<InputState>,
}

impl SectionAuth {
    pub fn new(username: Entity<InputState>, password: Entity<InputState>) -> Self {
        Self {
            username_input: username,
            password_input: password,
        }
    }
}

impl RenderOnce for SectionAuth {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        v_flex()
            .gap_2()
            .child(Input::new(&self.username_input).cleanable(true))
            .child(
                Input::new(&self.password_input)
                    .cleanable(true)
                    .mask_toggle(),
            )
    }
}
