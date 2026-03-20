use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::{Disableable as _, Sizable as _, button::Button, h_flex, spinner::Spinner};

use super::TestStatus;

#[derive(IntoElement)]
pub(crate) struct SectionTest {
    test_status: TestStatus,
    on_test: Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>,
}

impl SectionTest {
    pub fn new(
        test_status: TestStatus,
        on_test: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            test_status,
            on_test: Box::new(on_test),
        }
    }
}

impl RenderOnce for SectionTest {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let is_testing = matches!(self.test_status, TestStatus::Testing);

        h_flex()
            .gap_2()
            .items_center()
            .flex_shrink_0()
            .child(
                Button::new("test-btn")
                    .label("Test Connection")
                    .outline()
                    .on_click(move |ev, window, cx| (self.on_test)(ev, window, cx))
                    .disabled(is_testing),
            )
            .when(is_testing, |this| {
                this.child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(Spinner::new().small())
                        .child("Testing..."),
                )
            })
    }
}
