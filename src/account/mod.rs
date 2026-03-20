use crate::config::{self, AccountConfig, AppConfig};
use crate::credentials::{self, CredentialKind};
use crate::error::{Error, Result};
use crate::mail::{imap::ImapClient, smtp::SmtpClient};

pub struct AccountManager {
    config: AppConfig,
}

impl AccountManager {
    pub fn load() -> Result<Self> {
        let config = config::load()?;
        Ok(Self { config })
    }

    pub fn find_account(&self, id: &str) -> Result<&AccountConfig> {
        self.config
            .accounts
            .iter()
            .find(|a| a.id == id)
            .ok_or_else(|| Error::Config(format!("account not found: {id}")))
    }

    pub fn upsert_account(&mut self, account: AccountConfig) -> Result<()> {
        if let Some(existing) = self.config.accounts.iter_mut().find(|a| a.id == account.id) {
            *existing = account;
        } else {
            self.config.accounts.push(account);
        }
        config::save(&self.config)
    }

    pub async fn remove_account(&mut self, account_id: &str) -> Result<()> {
        self.config.accounts.retain(|a| a.id != account_id);
        config::save(&self.config)?;

        // Best-effort cleanup of all credential kinds
        for kind in [
            CredentialKind::ImapPassword,
            CredentialKind::SmtpPassword,
            CredentialKind::OAuth2Token,
        ] {
            let _ = credentials::delete(account_id, kind).await;
        }

        Ok(())
    }

    pub async fn imap_session(&self, account_id: &str) -> Result<ImapClient> {
        let account = self.find_account(account_id)?;
        let username = account.imap.username.as_deref().unwrap_or(&account.email);
        let password = credentials::retrieve(account_id, CredentialKind::ImapPassword).await?;
        ImapClient::connect(&account.imap, username, &password).await
    }

    pub async fn smtp_client(&self, account_id: &str) -> Result<SmtpClient> {
        let account = self.find_account(account_id)?;
        let username = account.smtp.username.as_deref().unwrap_or(&account.email);
        let password = credentials::retrieve(account_id, CredentialKind::SmtpPassword).await?;
        SmtpClient::connect(&account.smtp, username, &password).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AuthMethod, ServerConfig, TlsMode};

    fn test_account(id: &str) -> AccountConfig {
        AccountConfig {
            id: id.into(),
            email: format!("{id}@example.com"),
            display_name: id.into(),
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
        }
    }

    #[test]
    fn find_account_returns_match() {
        let manager = AccountManager {
            config: AppConfig {
                accounts: vec![test_account("work"), test_account("personal")],
                ..Default::default()
            },
        };
        let found = manager.find_account("personal").unwrap();
        assert_eq!(found.id, "personal");
    }

    #[test]
    fn find_account_returns_error_for_missing() {
        let manager = AccountManager {
            config: AppConfig {
                accounts: vec![test_account("work")],
                ..Default::default()
            },
        };
        assert!(manager.find_account("nonexistent").is_err());
    }

    #[test]
    fn upsert_updates_existing() {
        let mut config = AppConfig {
            accounts: vec![test_account("work")],
            ..Default::default()
        };
        let mut updated = test_account("work");
        updated.display_name = "Updated Name".into();

        if let Some(existing) = config.accounts.iter_mut().find(|a| a.id == updated.id) {
            *existing = updated;
        }
        assert_eq!(config.accounts.len(), 1);
        assert_eq!(config.accounts[0].display_name, "Updated Name");
    }

    #[test]
    fn upsert_adds_new() {
        let mut config = AppConfig {
            accounts: vec![test_account("work")],
            ..Default::default()
        };
        let new = test_account("personal");
        if !config.accounts.iter().any(|a| a.id == new.id) {
            config.accounts.push(new);
        }
        assert_eq!(config.accounts.len(), 2);
    }
}
