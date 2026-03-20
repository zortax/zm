use gpui::*;
use gpui_component::{
    h_flex,
    input::{Input, InputState},
    select::{Select, SelectItem, SelectState},
};

use crate::config::TlsMode;

#[derive(Clone, PartialEq)]
pub struct TlsOption {
    pub label: SharedString,
    pub mode: TlsMode,
}

impl SelectItem for TlsOption {
    type Value = TlsMode;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &TlsMode {
        &self.mode
    }
}

pub fn tls_options() -> Vec<TlsOption> {
    vec![
        TlsOption {
            label: "TLS (Implicit)".into(),
            mode: TlsMode::Tls,
        },
        TlsOption {
            label: "STARTTLS".into(),
            mode: TlsMode::StartTls,
        },
        TlsOption {
            label: "None (Plaintext)".into(),
            mode: TlsMode::None,
        },
    ]
}

#[derive(IntoElement)]
pub struct ServerFields {
    host: Entity<InputState>,
    port: Entity<InputState>,
    tls: Entity<SelectState<Vec<TlsOption>>>,
}

impl ServerFields {
    pub fn new(
        host: Entity<InputState>,
        port: Entity<InputState>,
        tls: Entity<SelectState<Vec<TlsOption>>>,
    ) -> Self {
        Self { host, port, tls }
    }
}

impl RenderOnce for ServerFields {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        h_flex()
            .gap_3()
            .child(div().flex_1().child(Input::new(&self.host)))
            .child(div().w(px(80.)).child(Input::new(&self.port)))
            .child(div().w(px(140.)).child(Select::new(&self.tls)))
    }
}
