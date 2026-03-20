use gpui::*;
use gpui_component::{divider::Divider, v_flex};

use crate::components::server_fields::{ServerFields, TlsOption};

#[derive(IntoElement)]
pub(crate) struct SectionServers {
    imap_host: Entity<gpui_component::input::InputState>,
    imap_port: Entity<gpui_component::input::InputState>,
    imap_tls: Entity<gpui_component::select::SelectState<Vec<TlsOption>>>,
    smtp_host: Entity<gpui_component::input::InputState>,
    smtp_port: Entity<gpui_component::input::InputState>,
    smtp_tls: Entity<gpui_component::select::SelectState<Vec<TlsOption>>>,
}

impl SectionServers {
    pub fn new(
        imap_host: Entity<gpui_component::input::InputState>,
        imap_port: Entity<gpui_component::input::InputState>,
        imap_tls: Entity<gpui_component::select::SelectState<Vec<TlsOption>>>,
        smtp_host: Entity<gpui_component::input::InputState>,
        smtp_port: Entity<gpui_component::input::InputState>,
        smtp_tls: Entity<gpui_component::select::SelectState<Vec<TlsOption>>>,
    ) -> Self {
        Self {
            imap_host,
            imap_port,
            imap_tls,
            smtp_host,
            smtp_port,
            smtp_tls,
        }
    }
}

impl RenderOnce for SectionServers {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        v_flex()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child("IMAP (Incoming)"),
            )
            .child(ServerFields::new(
                self.imap_host,
                self.imap_port,
                self.imap_tls,
            ))
            .child(Divider::horizontal())
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child("SMTP (Outgoing)"),
            )
            .child(ServerFields::new(
                self.smtp_host,
                self.smtp_port,
                self.smtp_tls,
            ))
    }
}
