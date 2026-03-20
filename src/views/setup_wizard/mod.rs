mod section_auth;
mod section_basics;
mod section_servers;
mod section_test;

use gpui::*;
use gpui_component::{
    ActiveTheme, IndexPath,
    button::{Button, ButtonVariants as _},
    divider::Divider,
    h_flex,
    input::{InputEvent, InputState},
    select::SelectState,
    v_flex,
};
use gpui_tokio::Tokio;

use crate::components::server_fields::{TlsOption, tls_options};
use crate::config::{AccountConfig, AuthMethod, ServerConfig, TlsMode};
use crate::credentials::{self, CredentialKind};
use crate::mail::imap::ImapClient;
use crate::mail::smtp::SmtpClient;

pub struct AccountSaved;

impl EventEmitter<AccountSaved> for SetupWizard {}

use section_auth::SectionAuth;
use section_basics::SectionBasics;
use section_servers::SectionServers;
use section_test::SectionTest;

#[derive(Debug, Clone)]
pub(crate) enum TestStatus {
    Idle,
    Testing,
    Success,
    Failed(String),
}

pub struct SetupWizard {
    email_input: Entity<InputState>,
    display_name_input: Entity<InputState>,

    imap_host: Entity<InputState>,
    imap_port: Entity<InputState>,
    imap_tls: Entity<SelectState<Vec<TlsOption>>>,
    smtp_host: Entity<InputState>,
    smtp_port: Entity<InputState>,
    smtp_tls: Entity<SelectState<Vec<TlsOption>>>,

    username_input: Entity<InputState>,
    password_input: Entity<InputState>,

    test_status: TestStatus,
    _task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl SetupWizard {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let email_input = cx.new(|cx| InputState::new(window, cx).placeholder("you@example.com"));
        let display_name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Your Name"));

        let imap_host = cx.new(|cx| InputState::new(window, cx).placeholder("imap.example.com"));
        let imap_port = cx.new(|cx| InputState::new(window, cx).default_value("993"));
        let imap_tls =
            cx.new(|cx| SelectState::new(tls_options(), Some(IndexPath::default()), window, cx));

        let smtp_host = cx.new(|cx| InputState::new(window, cx).placeholder("smtp.example.com"));
        let smtp_port = cx.new(|cx| InputState::new(window, cx).default_value("465"));
        let smtp_tls =
            cx.new(|cx| SelectState::new(tls_options(), Some(IndexPath::default()), window, cx));

        let username_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Username (leave empty to use email address)")
        });
        let password_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Password")
                .masked(true)
        });

        let email_sub = cx.subscribe_in(&email_input, window, Self::on_email_change);

        Self {
            email_input,
            display_name_input,
            imap_host,
            imap_port,
            imap_tls,
            smtp_host,
            smtp_port,
            smtp_tls,
            username_input,
            password_input,
            test_status: TestStatus::Idle,
            _task: None,
            _subscriptions: vec![email_sub],
        }
    }

    fn on_email_change(
        &mut self,
        input: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event, InputEvent::Change) {
            return;
        }

        let email = input.read(cx).value();
        let domain = match email.split('@').nth(1) {
            Some(d) if !d.is_empty() => d.to_string(),
            _ => return,
        };

        self.imap_host.update(cx, |s, cx| {
            s.set_value(format!("imap.{domain}"), window, cx);
        });
        self.smtp_host.update(cx, |s, cx| {
            s.set_value(format!("smtp.{domain}"), window, cx);
        });
    }

    fn test_connection(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.test_status = TestStatus::Testing;
        cx.notify();

        let config = self.build_account_config(cx);
        let password = self.password_input.read(cx).value().to_string();
        let imap_user = config
            .imap
            .username
            .clone()
            .unwrap_or_else(|| config.email.clone());
        let smtp_user = config
            .smtp
            .username
            .clone()
            .unwrap_or_else(|| config.email.clone());

        self._task = Some(cx.spawn(async move |this, cx| {
            let result = Tokio::spawn(cx, async move {
                let imap = ImapClient::connect(&config.imap, &imap_user, &password).await?;
                imap.logout().await?;
                let _smtp = SmtpClient::connect(&config.smtp, &smtp_user, &password).await?;
                Ok::<(), crate::error::Error>(())
            })
            .await;

            this.update(cx, |this, cx| {
                this.test_status = match result {
                    Ok(Ok(())) => TestStatus::Success,
                    Ok(Err(e)) => TestStatus::Failed(e.to_string()),
                    Err(e) => TestStatus::Failed(e.to_string()),
                };
                cx.notify();
            })
            .ok();
        }));
    }

    fn save_account(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let config = self.build_account_config(cx);
        let password = self.password_input.read(cx).value().to_string();

        self._task = Some(cx.spawn(async move |_this, cx| {
            let result = Tokio::spawn(cx, async move {
                let mut mgr = crate::account::AccountManager::load()?;
                mgr.upsert_account(config.clone())?;
                credentials::store(&config.id, CredentialKind::ImapPassword, &password).await?;
                credentials::store(&config.id, CredentialKind::SmtpPassword, &password).await?;
                Ok::<(), crate::error::Error>(())
            })
            .await;

            match result {
                Ok(Ok(())) => {
                    _this
                        .update(cx, |_this, cx| {
                            cx.emit(AccountSaved);
                        })
                        .ok();
                }
                Ok(Err(e)) => {
                    tracing::error!("Failed to save account: {e}");
                }
                Err(e) => {
                    tracing::error!("Failed to save account: {e}");
                }
            }
        }));
    }

    fn build_account_config(&self, cx: &App) -> AccountConfig {
        let email = self.email_input.read(cx).value().to_string();
        let username_raw = self.username_input.read(cx).value().to_string();
        let username = if username_raw.trim().is_empty() {
            None
        } else {
            Some(username_raw.trim().to_string())
        };
        let imap_tls = self
            .imap_tls
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or(TlsMode::Tls);
        let smtp_tls = self
            .smtp_tls
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or(TlsMode::Tls);

        AccountConfig {
            id: email.clone(),
            email,
            display_name: self.display_name_input.read(cx).value().to_string(),
            imap: ServerConfig {
                host: self.imap_host.read(cx).value().to_string(),
                port: self.imap_port.read(cx).value().parse().unwrap_or(993),
                tls: imap_tls,
                username: username.clone(),
            },
            smtp: ServerConfig {
                host: self.smtp_host.read(cx).value().to_string(),
                port: self.smtp_port.read(cx).value().parse().unwrap_or(465),
                tls: smtp_tls,
                username,
            },
            auth: AuthMethod::Password,
            sync_interval_secs: 300,
        }
    }
}

impl Render for SetupWizard {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                v_flex()
                    .w(px(480.))
                    .gap_4()
                    .p_6()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::BOLD)
                            .child("Add Account"),
                    )
                    .child(SectionBasics::new(
                        self.email_input.clone(),
                        self.display_name_input.clone(),
                    ))
                    .child(Divider::horizontal())
                    .child(SectionServers::new(
                        self.imap_host.clone(),
                        self.imap_port.clone(),
                        self.imap_tls.clone(),
                        self.smtp_host.clone(),
                        self.smtp_port.clone(),
                        self.smtp_tls.clone(),
                    ))
                    .child(Divider::horizontal())
                    .child(SectionAuth::new(
                        self.username_input.clone(),
                        self.password_input.clone(),
                    ))
                    .child(Divider::horizontal())
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                h_flex()
                                    .w_full()
                                    .justify_between()
                                    .items_center()
                                    .child(SectionTest::new(
                                        self.test_status.clone(),
                                        cx.listener(Self::test_connection),
                                    ))
                                    .child(
                                        Button::new("save")
                                            .label("Add Account")
                                            .primary()
                                            .flex_shrink_0()
                                            .on_click(cx.listener(Self::save_account)),
                                    ),
                            )
                            .children(match &self.test_status {
                                TestStatus::Success => {
                                    let theme = cx.theme();
                                    Some(
                                        div()
                                            .text_sm()
                                            .text_color(theme.success)
                                            .child("Connection successful!"),
                                    )
                                }
                                TestStatus::Failed(err) => {
                                    let theme = cx.theme();
                                    Some(
                                        div()
                                            .text_sm()
                                            .text_color(theme.danger)
                                            .child(format!("Error: {err}")),
                                    )
                                }
                                _ => None,
                            }),
                    ),
            )
    }
}
