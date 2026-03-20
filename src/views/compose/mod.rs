use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme,
    dialog::{DialogHeader, DialogTitle},
    divider::Divider,
    input::{Input, InputState},
    v_flex,
};
use gpui_tokio::Tokio;
use lettre::message::Mailbox;
use sqlx::SqlitePool;

use crate::actions::ops;
use crate::components::compose_actions::ComposeActions;
use crate::components::compose_metadata::ComposeMetadata;
use crate::error::Result;
use crate::state::mail::MailMessage;

/// Events emitted by ComposeView.
pub enum ComposeEvent {
    Sent,
    DraftSaved,
    Discarded,
}

impl EventEmitter<ComposeEvent> for ComposeView {}

/// Identifies the original draft so it can be deleted after sending.
#[derive(Clone)]
struct DraftOrigin {
    mailbox_name: String,
    uid: u32,
    db_id: i64,
}

pub struct ComposeView {
    account_id: String,
    from_email: String,
    from_input: Entity<InputState>,
    to_input: Entity<InputState>,
    subject_input: Entity<InputState>,
    body_input: Entity<InputState>,
    sending: bool,
    error: Option<String>,
    pool: Option<SqlitePool>,
    /// Set when editing an existing draft, so we can delete it after send.
    draft_origin: Option<DraftOrigin>,
    /// Tracks whether an event was already emitted (to prevent double-save on close).
    resolved: bool,
    _task: Option<Task<()>>,
}

impl ComposeView {
    pub fn new(
        account_id: String,
        from_email: String,
        pool: Option<SqlitePool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let from_display: SharedString = from_email.clone().into();

        let from_input = cx.new(|cx| InputState::new(window, cx).default_value(from_display));
        let to_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("recipient@example.com"));
        let subject_input = cx.new(|cx| InputState::new(window, cx).placeholder("Subject"));
        let body_input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .placeholder("Write your message...")
        });

        Self {
            account_id,
            from_email,
            from_input,
            to_input,
            subject_input,
            body_input,
            sending: false,
            error: None,
            pool,
            draft_origin: None,
            resolved: false,
            _task: None,
        }
    }

    /// Create a compose view pre-filled from a draft message.
    pub fn from_draft(
        draft: &MailMessage,
        pool: Option<SqlitePool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let from_display: SharedString =
            format!("{} <{}>", draft.from_name, draft.from_email).into();

        let from_input = cx.new(|cx| InputState::new(window, cx).default_value(from_display));
        let to_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(SharedString::from(draft.to.join(", ")))
                .placeholder("recipient@example.com")
        });
        let subject_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(SharedString::from(draft.subject.clone()))
                .placeholder("Subject")
        });
        let body_input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .default_value(SharedString::from(draft.body.clone()))
                .placeholder("Write your message...")
        });

        let draft_origin = Some(DraftOrigin {
            mailbox_name: draft.mailbox_name.clone(),
            uid: draft.uid as u32,
            db_id: draft.id,
        });

        Self {
            account_id: draft.account_id.clone(),
            from_email: draft.from_email.clone(),
            from_input,
            to_input,
            subject_input,
            body_input,
            sending: false,
            error: None,
            pool,
            draft_origin,
            resolved: false,
            _task: None,
        }
    }

    /// Returns true if any compose field has content.
    pub fn has_content(&self, cx: &App) -> bool {
        !self.to_input.read(cx).value().is_empty()
            || !self.subject_input.read(cx).value().is_empty()
            || !self.body_input.read(cx).value().is_empty()
    }

    /// Build a lettre Message from the current input values.
    fn build_message(&self, cx: &App) -> Result<lettre::Message> {
        let from: Mailbox = self.from_email.parse()?;
        let to_text = self.to_input.read(cx).value().to_string();
        let to: Mailbox = to_text.parse()?;
        let subject = self.subject_input.read(cx).value().to_string();
        let body = self.body_input.read(cx).value().to_string();

        let message = lettre::Message::builder()
            .message_id(None)
            .from(from)
            .to(to)
            .subject(subject)
            .body(body)?;

        Ok(message)
    }

    fn send(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let message = match self.build_message(cx) {
            Ok(m) => m,
            Err(e) => {
                self.error = Some(e.to_string());
                cx.notify();
                return;
            }
        };

        self.sending = true;
        self.error = None;
        cx.notify();

        let account_id = self.account_id.clone();
        let draft_origin = self.draft_origin.clone();
        let pool = self.pool.clone();
        self._task = Some(cx.spawn(async move |this, cx| {
            let result = Tokio::spawn(cx, {
                let account_id = account_id.clone();
                async move { ops::send_message(&account_id, message).await }
            })
            .await;

            // If send succeeded and this was an existing draft, delete it
            if matches!(result, Ok(Ok(()))) {
                if let (Some(origin), Some(pool)) = (draft_origin, pool) {
                    let _ = Tokio::spawn(cx, {
                        let account_id = account_id.clone();
                        async move {
                            ops::delete_draft(
                                &account_id,
                                &origin.mailbox_name,
                                origin.uid,
                                origin.db_id,
                                &pool,
                            )
                            .await
                        }
                    })
                    .await;
                }
            }

            let _ = this.update(cx, |this, cx| {
                this.sending = false;
                match result {
                    Ok(Ok(())) => {
                        this.resolved = true;
                        cx.emit(ComposeEvent::Sent);
                    }
                    Ok(Err(e)) => {
                        this.error = Some(e.to_string());
                    }
                    Err(e) => {
                        this.error = Some(e.to_string());
                    }
                }
                cx.notify();
            });
        }));
    }

    pub fn save_draft(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.resolved || !self.has_content(cx) {
            return;
        }

        let message = match self.build_message(cx) {
            Ok(m) => m,
            Err(e) => {
                self.error = Some(e.to_string());
                cx.notify();
                return;
            }
        };

        let rfc822_bytes = message.formatted();
        let account_id = self.account_id.clone();
        let pool = self.pool.clone();

        self._task = Some(cx.spawn(async move |this, cx| {
            let result = if let Some(pool) = pool {
                Tokio::spawn(cx, async move {
                    ops::append_to_drafts(&account_id, rfc822_bytes, &pool).await
                })
                .await
            } else {
                Ok(Err(crate::error::Error::Config(
                    "no database connection".into(),
                )))
            };

            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(())) => {
                        this.resolved = true;
                        cx.emit(ComposeEvent::DraftSaved);
                    }
                    Ok(Err(e)) => {
                        tracing::error!(error = %e, "failed to save draft");
                        this.error = Some(e.to_string());
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "draft save task failed");
                        this.error = Some(e.to_string());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn discard(&mut self, cx: &mut Context<Self>) {
        self.resolved = true;
        cx.emit(ComposeEvent::Discarded);
    }
}

impl Render for ComposeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let danger_color = cx.theme().danger;

        v_flex()
            .size_full()
            .child(
                DialogHeader::new()
                    .p_4()
                    .pb_2()
                    .child(DialogTitle::new().child("New Message")),
            )
            .child(div().px_4().child(Divider::horizontal()))
            .child(div().p_4().child(ComposeMetadata::new(
                &self.from_input,
                &self.to_input,
                &self.subject_input,
            )))
            .child(div().px_4().child(Divider::horizontal()))
            .child(
                div().flex_1().p_4().min_h_0().child(
                    div()
                        .size_full()
                        .child(Input::new(&self.body_input).appearance(false).h_full()),
                ),
            )
            .when_some(self.error.as_ref(), |el, err| {
                el.child(
                    div()
                        .px_4()
                        .pb_2()
                        .text_sm()
                        .text_color(danger_color)
                        .child(err.clone()),
                )
            })
            .child(div().px_4().child(Divider::horizontal()))
            .child(div().p_4().child(ComposeActions::new(
                cx.listener(|this, _, window, cx| this.send(window, cx)),
                cx.listener(|this, _, window, cx| this.save_draft(window, cx)),
                cx.listener(|this, _, _, cx| this.discard(cx)),
                self.sending,
            )))
    }
}
