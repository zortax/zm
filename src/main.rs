mod account;
mod actions;
mod assets;
mod components;
mod config;
mod credentials;
mod db;
mod error;
mod mail;
mod search;
mod state;
mod sync;
mod views;

use gpui::*;
use gpui_component::Root;
use gpui_component::theme::{Theme, ThemeMode, ThemeRegistry};
use gpui_platform::application;
use tracing_subscriber::{EnvFilter, fmt};

use views::inbox::{InboxView, NoAccountsRemaining};
use views::setup_wizard::{AccountSaved, SetupWizard};

enum AppScreen {
    Setup(Entity<SetupWizard>),
    Inbox(Entity<InboxView>),
}

struct AppView {
    screen: AppScreen,
    _subscriptions: Vec<Subscription>,
}

impl AppView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let has_accounts = config::load()
            .map(|c| !c.accounts.is_empty())
            .unwrap_or(false);

        let mut subs = Vec::new();

        let screen = if has_accounts {
            let inbox = cx.new(|cx| InboxView::new(window, cx));
            subs.push(cx.subscribe_in(&inbox, window, Self::on_no_accounts));
            AppScreen::Inbox(inbox)
        } else {
            let wizard = cx.new(|cx| SetupWizard::new(window, cx));
            subs.push(cx.subscribe_in(&wizard, window, Self::on_account_saved));
            AppScreen::Setup(wizard)
        };

        Self {
            screen,
            _subscriptions: subs,
        }
    }

    fn on_account_saved(
        &mut self,
        _wizard: &Entity<SetupWizard>,
        _event: &AccountSaved,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let inbox = cx.new(|cx| InboxView::new(window, cx));
        self._subscriptions
            .push(cx.subscribe_in(&inbox, window, Self::on_no_accounts));
        self.screen = AppScreen::Inbox(inbox);
        cx.notify();
    }

    fn on_no_accounts(
        &mut self,
        _inbox: &Entity<InboxView>,
        _event: &NoAccountsRemaining,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let wizard = cx.new(|cx| SetupWizard::new(window, cx));
        self._subscriptions
            .push(cx.subscribe_in(&wizard, window, Self::on_account_saved));
        self.screen = AppScreen::Setup(wizard);
        cx.notify();
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(match &self.screen {
                AppScreen::Setup(view) => view.clone().into_any_element(),
                AppScreen::Inbox(view) => view.clone().into_any_element(),
            })
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
    }
}

fn main() {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("zm=info".parse().unwrap()))
        .with_target(true)
        .init();

    tracing::info!("starting zm");

    let app = application()
        .with_assets(gpui_component_assets::Assets)
        .with_assets(assets::Assets);

    app.run(move |cx: &mut App| {
        gpui_component::init(cx);
        gpui_tokio::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);

        // Load all embedded themes into the registry
        let registry = ThemeRegistry::global_mut(cx);
        for path in assets::Assets::iter() {
            if path.starts_with("themes/") && path.ends_with(".json") {
                if let Some(data) = assets::Assets::get(&path) {
                    if let Ok(content) = std::str::from_utf8(&data.data) {
                        if let Err(e) = registry.load_themes_from_str(content) {
                            tracing::error!("Failed to load theme {}: {}", path, e);
                        }
                    }
                }
            }
        }

        // Initialize global settings from config and apply the saved theme
        let app_config = config::load().unwrap_or_default();
        cx.set_global(config::ZmSettings::from_config(&app_config));

        let theme_name = &app_config.general.theme;
        if let Some(theme_config) = ThemeRegistry::global(cx)
            .themes()
            .get(theme_name.as_str())
            .cloned()
        {
            Theme::change(theme_config.mode, None, cx);
            Theme::global_mut(cx).apply_config(&theme_config);
        } else if let Some(fallback) = ThemeRegistry::global(cx)
            .themes()
            .get("Jellybeans")
            .cloned()
        {
            Theme::global_mut(cx).apply_config(&fallback);
        }

        cx.open_window(WindowOptions::default(), |window, cx| {
            let view = cx.new(|cx| AppView::new(window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        })
        .unwrap();

        cx.activate(true);
    });
}
