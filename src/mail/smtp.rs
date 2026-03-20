use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::{ServerConfig, TlsMode};
use crate::error::Result;

pub struct SmtpClient {
    transport: AsyncSmtpTransport<Tokio1Executor>,
}

impl SmtpClient {
    pub async fn connect(server: &ServerConfig, username: &str, password: &str) -> Result<Self> {
        let creds = Credentials::new(username.to_owned(), password.to_owned());

        let transport = match server.tls {
            TlsMode::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&server.host)?
                .port(server.port)
                .credentials(creds)
                .build(),
            TlsMode::StartTls => {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&server.host)?
                    .port(server.port)
                    .credentials(creds)
                    .build()
            }
            TlsMode::None => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&server.host)
                .port(server.port)
                .credentials(creds)
                .build(),
        };

        Ok(Self { transport })
    }

    pub async fn send(&self, message: Message) -> Result<()> {
        self.transport.send(message).await?;
        Ok(())
    }
}
