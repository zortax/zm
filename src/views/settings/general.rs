use gpui::{App, Entity, ParentElement, SharedString, Styled};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable,
    button::Button,
    h_flex,
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage},
    theme::{Theme, ThemeMode, ThemeRegistry},
};

use super::SettingsView;
use crate::config::ZmSettings;

pub fn build_page(view: &Entity<SettingsView>, cx: &App) -> SettingPage {
    SettingPage::new("General")
        .resettable(false)
        .default_open(true)
        .icon(Icon::new(IconName::Settings2))
        .groups(vec![appearance_group(cx), font_group(), search_group(view)])
}

fn appearance_group(cx: &App) -> SettingGroup {
    // Build theme dropdown options from the registry
    let mut theme_options: Vec<(SharedString, SharedString)> = ThemeRegistry::global(cx)
        .themes()
        .keys()
        .map(|name| (name.clone(), name.clone()))
        .collect();
    theme_options.sort_by(|a, b| a.1.cmp(&b.1));

    SettingGroup::new().title("Appearance").items(vec![
        SettingItem::new(
            "Theme",
            SettingField::dropdown(
                theme_options,
                |cx: &App| ZmSettings::global(cx).theme.clone(),
                |val: SharedString, cx: &mut App| {
                    ZmSettings::global_mut(cx).theme = val.clone();
                    // Apply the selected theme
                    if let Some(theme_config) =
                        ThemeRegistry::global(cx).themes().get(&val).cloned()
                    {
                        Theme::change(theme_config.mode, None, cx);
                        Theme::global_mut(cx).apply_config(&theme_config);
                    }
                    ZmSettings::save(cx);
                },
            ),
        )
        .description("Select the color theme for the application."),
        SettingItem::new(
            "Dark Mode",
            SettingField::switch(
                |cx: &App| cx.theme().mode.is_dark(),
                |val: bool, cx: &mut App| {
                    let mode = if val {
                        ThemeMode::Dark
                    } else {
                        ThemeMode::Light
                    };
                    Theme::change(mode, None, cx);
                },
            )
            .default_value(false),
        )
        .description("Switch between light and dark modes."),
    ])
}

fn font_group() -> SettingGroup {
    let default = crate::config::GeneralConfig::default();

    SettingGroup::new().title("Font").items(vec![
        SettingItem::new(
            "Font Family",
            SettingField::dropdown(
                vec![
                    ("Sans Serif".into(), "Sans Serif".into()),
                    ("Monospace".into(), "Monospace".into()),
                    ("Arial".into(), "Arial".into()),
                    ("Helvetica".into(), "Helvetica".into()),
                    ("Times New Roman".into(), "Times New Roman".into()),
                    ("Courier New".into(), "Courier New".into()),
                ],
                |cx: &App| ZmSettings::global(cx).font_family.clone(),
                |val: SharedString, cx: &mut App| {
                    ZmSettings::global_mut(cx).font_family = val;
                    ZmSettings::save(cx);
                },
            )
            .default_value(default.font_family),
        )
        .description("Select the font family for the application."),
        SettingItem::new(
            "Font Size",
            SettingField::number_input(
                NumberFieldOptions {
                    min: 8.0,
                    max: 32.0,
                    ..Default::default()
                },
                |cx: &App| ZmSettings::global(cx).font_size,
                |val: f64, cx: &mut App| {
                    ZmSettings::global_mut(cx).font_size = val;
                    ZmSettings::save(cx);
                },
            )
            .default_value(default.font_size),
        )
        .description("Adjust the font size (8–32)."),
        SettingItem::new(
            "Line Height",
            SettingField::number_input(
                NumberFieldOptions {
                    min: 1.0,
                    max: 3.0,
                    step: 0.1,
                    ..Default::default()
                },
                |cx: &App| ZmSettings::global(cx).line_height,
                |val: f64, cx: &mut App| {
                    ZmSettings::global_mut(cx).line_height = val;
                    ZmSettings::save(cx);
                },
            )
            .default_value(default.line_height),
        )
        .description("Adjust the line height multiplier (1.0–3.0)."),
    ])
}

fn search_group(view: &Entity<SettingsView>) -> SettingGroup {
    let view = view.clone();
    let view_for_model = view.clone();
    SettingGroup::new().title("Semantic Search").items(vec![
        SettingItem::new(
            "Enable Semantic Search",
            SettingField::switch(
                |cx: &App| ZmSettings::global(cx).semantic_search_enabled,
                |val: bool, cx: &mut App| {
                    ZmSettings::global_mut(cx).semantic_search_enabled = val;
                    ZmSettings::save(cx);
                },
            )
            .default_value(false),
        )
        .description("Use AI embeddings for semantic mail search across all folders."),
        SettingItem::new(
            "Embedding Model",
            SettingField::dropdown(
                vec![
                    (
                        SharedString::from("intfloat/multilingual-e5-small"),
                        "Multilingual E5 Small (384d, multi-language)".into(),
                    ),
                    (
                        SharedString::from("BAAI/bge-small-en-v1.5"),
                        "BGE Small EN v1.5 (384d, English)".into(),
                    ),
                    (
                        SharedString::from("sentence-transformers/all-MiniLM-L6-v2"),
                        "MiniLM L6 v2 (384d, English, fastest)".into(),
                    ),
                ],
                |cx: &App| ZmSettings::global(cx).embedding_model.clone(),
                {
                    let view = view_for_model;
                    move |val: SharedString, cx: &mut App| {
                        let old = ZmSettings::global(cx).embedding_model.clone();
                        ZmSettings::global_mut(cx).embedding_model = val.clone();
                        ZmSettings::save(cx);
                        if old != val {
                            view.update(cx, |_, cx| {
                                cx.emit(super::SettingsEvent::ModelChanged(val.to_string()));
                            });
                        }
                    }
                },
            ),
        )
        .description("Select the embedding model. Changing models requires re-indexing."),
        SettingItem::render(move |options, _, _cx| {
            let view = view.clone();
            h_flex().w_full().child(
                Button::new("reset-embeddings")
                    .icon(IconName::Replace)
                    .label("Reset Embeddings & Re-index")
                    .outline()
                    .with_size(options.size)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.reset_embeddings(cx);
                        });
                    }),
            )
        }),
    ])
}
