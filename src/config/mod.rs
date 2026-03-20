use std::fs;
use std::path::PathBuf;

use gpui::{Global, SharedString};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub semantic_search: SemanticSearchConfig,
    #[serde(default)]
    pub accounts: Vec<AccountConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SemanticSearchConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_embedding_model")]
    pub model: String,
}

impl Default for SemanticSearchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: default_embedding_model(),
        }
    }
}

fn default_embedding_model() -> String {
    "intfloat/multilingual-e5-small".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeneralConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_font_family")]
    pub font_family: String,
    #[serde(default = "default_font_size")]
    pub font_size: f64,
    #[serde(default = "default_line_height")]
    pub line_height: f64,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            font_family: default_font_family(),
            font_size: default_font_size(),
            line_height: default_line_height(),
        }
    }
}

fn default_theme() -> String {
    "Jellybeans".into()
}

fn default_font_family() -> String {
    "Sans Serif".into()
}

fn default_font_size() -> f64 {
    14.0
}

fn default_line_height() -> f64 {
    1.5
}

/// Global settings state accessible from gpui-component SettingField closures.
pub struct ZmSettings {
    pub theme: SharedString,
    pub font_family: SharedString,
    pub font_size: f64,
    pub line_height: f64,
    pub semantic_search_enabled: bool,
    pub embedding_model: SharedString,
}

impl Global for ZmSettings {}

impl ZmSettings {
    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            theme: config.general.theme.clone().into(),
            font_family: config.general.font_family.clone().into(),
            font_size: config.general.font_size,
            line_height: config.general.line_height,
            semantic_search_enabled: config.semantic_search.enabled,
            embedding_model: config.semantic_search.model.clone().into(),
        }
    }

    pub fn global(cx: &gpui::App) -> &Self {
        cx.global::<Self>()
    }

    pub fn global_mut(cx: &mut gpui::App) -> &mut Self {
        cx.global_mut::<Self>()
    }

    /// Persist the current global settings to disk.
    pub fn save(cx: &gpui::App) {
        let settings = Self::global(cx);
        if let Ok(mut config) = crate::config::load() {
            config.general.theme = settings.theme.to_string();
            config.general.font_family = settings.font_family.to_string();
            config.general.font_size = settings.font_size;
            config.general.line_height = settings.line_height;
            config.semantic_search.enabled = settings.semantic_search_enabled;
            config.semantic_search.model = settings.embedding_model.to_string();
            if let Err(e) = crate::config::save(&config) {
                tracing::error!("Failed to save config: {}", e);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccountConfig {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub imap: ServerConfig,
    pub smtp: ServerConfig,
    pub auth: AuthMethod,
    #[serde(default = "default_sync_interval")]
    pub sync_interval_secs: u64,
}

fn default_sync_interval() -> u64 {
    300
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub tls: TlsMode,
    /// Login username. Defaults to the account email if not set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TlsMode {
    Tls,
    StartTls,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthMethod {
    Password,
    #[serde(rename = "oauth2")]
    OAuth2 {
        client_id: String,
    },
}

pub fn config_path() -> Result<PathBuf> {
    dirs::config_dir()
        .map(|d| d.join("zm").join("config.toml"))
        .ok_or_else(|| Error::Config("could not determine config directory".into()))
}

pub fn load() -> Result<AppConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let contents = fs::read_to_string(&path)?;
    deserialize(&contents)
}

pub fn save(config: &AppConfig) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = serialize(config)?;
    fs::write(&path, contents)?;
    Ok(())
}

pub fn serialize(config: &AppConfig) -> Result<String> {
    Ok(toml::to_string_pretty(config)?)
}

pub fn deserialize(s: &str) -> Result<AppConfig> {
    Ok(toml::from_str(s)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> AppConfig {
        AppConfig {
            accounts: vec![AccountConfig {
                id: "work".into(),
                email: "user@example.com".into(),
                display_name: "Test User".into(),
                imap: ServerConfig {
                    host: "imap.example.com".into(),
                    port: 993,
                    tls: TlsMode::Tls,
                    username: None,
                },
                smtp: ServerConfig {
                    host: "smtp.example.com".into(),
                    port: 465,
                    tls: TlsMode::Tls,
                    username: None,
                },
                auth: AuthMethod::Password,
                sync_interval_secs: 300,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn round_trip_serde() {
        let config = sample_config();
        let serialized = serialize(&config).unwrap();
        let deserialized = deserialize(&serialized).unwrap();
        assert_eq!(config, deserialized);
    }

    #[test]
    fn round_trip_oauth2() {
        let config = AppConfig {
            accounts: vec![AccountConfig {
                id: "gmail".into(),
                email: "user@gmail.com".into(),
                display_name: "Gmail User".into(),
                imap: ServerConfig {
                    host: "imap.gmail.com".into(),
                    port: 993,
                    tls: TlsMode::Tls,
                    username: None,
                },
                smtp: ServerConfig {
                    host: "smtp.gmail.com".into(),
                    port: 587,
                    tls: TlsMode::StartTls,
                    username: None,
                },
                auth: AuthMethod::OAuth2 {
                    client_id: "abc123".into(),
                },
                sync_interval_secs: 300,
            }],
            ..Default::default()
        };
        let serialized = serialize(&config).unwrap();
        let deserialized = deserialize(&serialized).unwrap();
        assert_eq!(config, deserialized);
    }

    #[test]
    fn tls_mode_serialization() {
        let toml_str = serialize(&AppConfig {
            accounts: vec![AccountConfig {
                id: "test".into(),
                email: "t@t.com".into(),
                display_name: "T".into(),
                imap: ServerConfig {
                    host: "h".into(),
                    port: 993,
                    tls: TlsMode::StartTls,
                    username: None,
                },
                smtp: ServerConfig {
                    host: "h".into(),
                    port: 587,
                    tls: TlsMode::None,
                    username: None,
                },
                auth: AuthMethod::Password,
                sync_interval_secs: 300,
            }],
            ..Default::default()
        })
        .unwrap();

        assert!(toml_str.contains("\"start_tls\""));
        assert!(toml_str.contains("\"none\""));
    }

    #[test]
    fn auth_method_tagged_serialization() {
        let toml_str = serialize(&AppConfig {
            accounts: vec![AccountConfig {
                id: "test".into(),
                email: "t@t.com".into(),
                display_name: "T".into(),
                imap: ServerConfig {
                    host: "h".into(),
                    port: 993,
                    tls: TlsMode::Tls,
                    username: None,
                },
                smtp: ServerConfig {
                    host: "h".into(),
                    port: 465,
                    tls: TlsMode::Tls,
                    username: None,
                },
                auth: AuthMethod::OAuth2 {
                    client_id: "my-client".into(),
                },
                sync_interval_secs: 300,
            }],
            ..Default::default()
        })
        .unwrap();

        assert!(toml_str.contains("type = \"oauth2\""));
        assert!(toml_str.contains("client_id = \"my-client\""));
    }

    #[test]
    fn empty_config_deserialize() {
        let config = deserialize("").unwrap();
        assert!(config.accounts.is_empty());
    }

    #[test]
    fn config_path_is_under_config_dir() {
        let path = config_path().unwrap();
        let config_dir = dirs::config_dir().unwrap();
        assert!(path.starts_with(config_dir));
        assert!(path.ends_with("zm/config.toml"));
    }
}
