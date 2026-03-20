use gpui::*;
use gpui_component::{
    h_flex,
    input::{Input, InputState},
};

#[derive(IntoElement)]
pub(crate) struct SectionBasics {
    email_input: Entity<InputState>,
    display_name_input: Entity<InputState>,
}

impl SectionBasics {
    pub fn new(email: Entity<InputState>, display_name: Entity<InputState>) -> Self {
        Self {
            email_input: email,
            display_name_input: display_name,
        }
    }
}

impl RenderOnce for SectionBasics {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        h_flex()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .child(Input::new(&self.email_input).cleanable(true)),
            )
            .child(div().flex_1().child(Input::new(&self.display_name_input)))
    }
}
