use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialKind {
    ImapPassword,
    SmtpPassword,
    OAuth2Token,
}

impl CredentialKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::ImapPassword => "imap_password",
            Self::SmtpPassword => "smtp_password",
            Self::OAuth2Token => "oauth2_token",
        }
    }
}

pub fn service_key(account_id: &str, kind: CredentialKind) -> String {
    format!("zm:{account_id}:{}", kind.as_str())
}

pub async fn store(account_id: &str, kind: CredentialKind, secret: &str) -> Result<()> {
    let service = service_key(account_id, kind);
    let secret = secret.to_owned();
    tokio::task::spawn_blocking(move || {
        let entry = keyring::Entry::new(&service, &service)
            .map_err(|e| Error::Credential(e.to_string()))?;
        entry
            .set_password(&secret)
            .map_err(|e| Error::Credential(e.to_string()))
    })
    .await
    .map_err(|e| Error::Credential(e.to_string()))?
}

pub async fn retrieve(account_id: &str, kind: CredentialKind) -> Result<String> {
    let service = service_key(account_id, kind);
    tokio::task::spawn_blocking(move || {
        let entry = keyring::Entry::new(&service, &service)
            .map_err(|e| Error::Credential(e.to_string()))?;
        entry
            .get_password()
            .map_err(|e| Error::Credential(e.to_string()))
    })
    .await
    .map_err(|e| Error::Credential(e.to_string()))?
}

pub async fn delete(account_id: &str, kind: CredentialKind) -> Result<()> {
    let service = service_key(account_id, kind);
    tokio::task::spawn_blocking(move || {
        let entry = keyring::Entry::new(&service, &service)
            .map_err(|e| Error::Credential(e.to_string()))?;
        entry
            .delete_credential()
            .map_err(|e| Error::Credential(e.to_string()))
    })
    .await
    .map_err(|e| Error::Credential(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_key_format() {
        assert_eq!(
            service_key("work", CredentialKind::ImapPassword),
            "zm:work:imap_password"
        );
        assert_eq!(
            service_key("personal", CredentialKind::SmtpPassword),
            "zm:personal:smtp_password"
        );
        assert_eq!(
            service_key("gmail", CredentialKind::OAuth2Token),
            "zm:gmail:oauth2_token"
        );
    }

    #[test]
    fn service_key_with_special_chars() {
        assert_eq!(
            service_key("my-work-account", CredentialKind::ImapPassword),
            "zm:my-work-account:imap_password"
        );
    }
}
