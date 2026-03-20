use gpui::{App, Entity, IntoElement, ParentElement as _, SharedString, Styled};
use gpui_component::{
    Icon, IconName, Sizable,
    button::{Button, ButtonVariants as _},
    h_flex,
    setting::{SettingField, SettingGroup, SettingItem, SettingPage},
};

use crate::assets::IconName as AppIconName;
use crate::config::AccountConfig;

use super::SettingsView;

pub fn build_page(view: &Entity<SettingsView>, accounts: &[AccountConfig]) -> SettingPage {
    let groups: Vec<SettingGroup> = accounts
        .iter()
        .map(|account| build_account_group(view, account))
        .collect();

    SettingPage::new("Accounts")
        .resettable(false)
        .default_open(false)
        .icon(Icon::new(IconName::User))
        .groups(groups)
}

fn build_account_group(view: &Entity<SettingsView>, account: &AccountConfig) -> SettingGroup {
    let account_id = account.id.clone();

    let mut items = vec![
        SettingItem::new(
            "Display Name",
            account_input_field(&account_id, "display_name", &account.display_name),
        ),
        SettingItem::new(
            "Email",
            account_input_field(&account_id, "email", &account.email),
        ),
        SettingItem::new(
            "IMAP Host",
            account_input_field(&account_id, "imap_host", &account.imap.host),
        ),
        SettingItem::new(
            "IMAP Port",
            account_input_field(&account_id, "imap_port", &account.imap.port.to_string()),
        ),
        SettingItem::new(
            "SMTP Host",
            account_input_field(&account_id, "smtp_host", &account.smtp.host),
        ),
        SettingItem::new(
            "SMTP Port",
            account_input_field(&account_id, "smtp_port", &account.smtp.port.to_string()),
        ),
        SettingItem::new(
            "Sync Interval (seconds)",
            account_input_field(
                &account_id,
                "sync_interval",
                &account.sync_interval_secs.to_string(),
            ),
        ),
    ];

    // Action buttons: Reset DB & Re-sync, Delete Account
    let view_for_reset = view.clone();
    let id_for_reset = account_id.clone();
    let view_for_delete = view.clone();
    let id_for_delete = account_id.clone();

    items.push(SettingItem::render(move |options, _, _cx| {
        h_flex()
            .w_full()
            .justify_between()
            .gap_3()
            .child({
                let view = view_for_reset.clone();
                let id = id_for_reset.clone();
                Button::new("reset-db")
                    .icon(IconName::Replace)
                    .label("Reset DB & Re-sync")
                    .outline()
                    .with_size(options.size)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.reset_account_db(id.clone(), cx);
                        });
                    })
            })
            .child({
                let view = view_for_delete.clone();
                let id = id_for_delete.clone();
                Button::new("delete-account")
                    .icon(AppIconName::Trash2)
                    .label("Delete Account")
                    .danger()
                    .with_size(options.size)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.delete_account(id.clone(), cx);
                        });
                    })
            })
            .into_any_element()
    }));

    let title = format!("{} ({})", account.display_name, account.email);
    SettingGroup::new().title(title).items(items)
}

fn account_input_field(
    account_id: &str,
    field: &str,
    current_value: &str,
) -> SettingField<SharedString> {
    let account_id = account_id.to_string();
    let field = field.to_string();
    let default_value: SharedString = current_value.to_string().into();

    SettingField::input(
        {
            let account_id = account_id.clone();
            let field = field.clone();
            move |cx: &App| read_account_field(cx, &account_id, &field)
        },
        {
            let account_id = account_id.clone();
            let field = field.clone();
            move |val: SharedString, cx: &mut App| {
                write_account_field(cx, &account_id, &field, &val);
            }
        },
    )
    .default_value(default_value)
}

fn read_account_field(_cx: &App, account_id: &str, field: &str) -> SharedString {
    let config = crate::config::load().unwrap_or_default();
    let account = config.accounts.iter().find(|a| a.id == account_id);
    match (account, field) {
        (Some(a), "display_name") => a.display_name.clone().into(),
        (Some(a), "email") => a.email.clone().into(),
        (Some(a), "imap_host") => a.imap.host.clone().into(),
        (Some(a), "imap_port") => a.imap.port.to_string().into(),
        (Some(a), "smtp_host") => a.smtp.host.clone().into(),
        (Some(a), "smtp_port") => a.smtp.port.to_string().into(),
        (Some(a), "sync_interval") => a.sync_interval_secs.to_string().into(),
        _ => "".into(),
    }
}

fn write_account_field(_cx: &mut App, account_id: &str, field: &str, val: &str) {
    let Ok(mut config) = crate::config::load() else {
        return;
    };
    let Some(account) = config.accounts.iter_mut().find(|a| a.id == account_id) else {
        return;
    };

    match field {
        "display_name" => account.display_name = val.to_string(),
        "email" => account.email = val.to_string(),
        "imap_host" => account.imap.host = val.to_string(),
        "imap_port" => {
            if let Ok(port) = val.parse::<u16>() {
                account.imap.port = port;
            }
        }
        "smtp_host" => account.smtp.host = val.to_string(),
        "smtp_port" => {
            if let Ok(port) = val.parse::<u16>() {
                account.smtp.port = port;
            }
        }
        "sync_interval" => {
            if let Ok(secs) = val.parse::<u64>() {
                account.sync_interval_secs = secs;
            }
        }
        _ => return,
    }

    if let Err(e) = crate::config::save(&config) {
        tracing::error!("Failed to save account config: {}", e);
    }
}
