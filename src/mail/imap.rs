use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_imap::Session;
use futures::TryStreamExt;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

use chrono::{FixedOffset, TimeZone, Utc};

use crate::config::{ServerConfig, TlsMode};
use crate::error::{Error, Result};

/// Convert a mail-parser DateTime to a UTC RFC 3339 string for correct chronological sorting.
pub fn date_to_utc_rfc3339(d: &mail_parser::DateTime) -> String {
    let offset_secs =
        (d.tz_hour as i32 * 3600 + d.tz_minute as i32 * 60) * if d.tz_before_gmt { -1 } else { 1 };

    let Some(offset) = FixedOffset::east_opt(offset_secs) else {
        return d.to_rfc3339();
    };

    let Some(local) = offset
        .with_ymd_and_hms(
            d.year as i32,
            d.month as u32,
            d.day as u32,
            d.hour as u32,
            d.minute as u32,
            d.second as u32,
        )
        .single()
    else {
        return d.to_rfc3339();
    };

    local.with_timezone(&Utc).to_rfc3339()
}

/// A stream that is either plaintext TCP or TLS-wrapped.
#[derive(Debug)]
pub(crate) enum MaybeTlsStream {
    Plain(TcpStream),
    Tls(TlsStream<TcpStream>),
}

impl AsyncRead for MaybeTlsStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MaybeTlsStream::Plain(s) => Pin::new(s).poll_read(cx, buf),
            MaybeTlsStream::Tls(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for MaybeTlsStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            MaybeTlsStream::Plain(s) => Pin::new(s).poll_write(cx, buf),
            MaybeTlsStream::Tls(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MaybeTlsStream::Plain(s) => Pin::new(s).poll_flush(cx),
            MaybeTlsStream::Tls(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MaybeTlsStream::Plain(s) => Pin::new(s).poll_shutdown(cx),
            MaybeTlsStream::Tls(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

pub(crate) type ImapSession = Session<MaybeTlsStream>;

pub struct ImapClient {
    session: ImapSession,
}

#[derive(Debug)]
pub struct Mailbox {
    pub name: String,
    pub delimiter: Option<String>,
}

#[derive(Debug)]
pub struct FetchedMessage {
    pub uid: u32,
    pub subject: String,
    pub from_name: String,
    pub from_email: String,
    pub to: Vec<String>,
    pub date: String,
    pub body: String,
    pub is_read: bool,
    pub is_starred: bool,
}

impl ImapClient {
    /// Consume the client and return the raw IMAP session (for IDLE).
    pub fn into_session(self) -> ImapSession {
        self.session
    }

    pub async fn connect(server: &ServerConfig, username: &str, password: &str) -> Result<Self> {
        let stream = match server.tls {
            TlsMode::Tls => {
                MaybeTlsStream::Tls(Self::connect_tls(&server.host, server.port).await?)
            }
            TlsMode::StartTls => {
                MaybeTlsStream::Tls(Self::connect_starttls(&server.host, server.port).await?)
            }
            TlsMode::None => {
                MaybeTlsStream::Plain(Self::connect_plain(&server.host, server.port).await?)
            }
        };

        let client = async_imap::Client::new(stream);
        let session = client.login(username, password).await.map_err(|e| e.0)?;

        Ok(Self { session })
    }

    async fn connect_plain(host: &str, port: u16) -> Result<TcpStream> {
        TcpStream::connect((host, port)).await.map_err(io_err)
    }

    async fn connect_tls(host: &str, port: u16) -> Result<TlsStream<TcpStream>> {
        let tcp = TcpStream::connect((host, port)).await.map_err(io_err)?;
        let connector = tls_connector();
        let server_name = rustls::pki_types::ServerName::try_from(host.to_owned())
            .map_err(|e| Error::Config(format!("invalid server name: {e}")))?;
        connector.connect(server_name, tcp).await.map_err(io_err)
    }

    async fn connect_starttls(host: &str, port: u16) -> Result<TlsStream<TcpStream>> {
        let tcp = TcpStream::connect((host, port)).await.map_err(io_err)?;
        let connector = tls_connector();
        let server_name = rustls::pki_types::ServerName::try_from(host.to_owned())
            .map_err(|e| Error::Config(format!("invalid server name: {e}")))?;
        connector.connect(server_name, tcp).await.map_err(io_err)
    }

    pub async fn list_mailboxes(&mut self) -> Result<Vec<Mailbox>> {
        let names: Vec<_> = self
            .session
            .list(None, Some("*"))
            .await?
            .try_collect()
            .await?;
        Ok(names
            .into_iter()
            .map(|n| Mailbox {
                name: n.name().to_owned(),
                delimiter: n.delimiter().map(|c| c.to_string()),
            })
            .collect())
    }

    /// Fetch all UIDs in a mailbox.
    pub async fn fetch_uids(&mut self, mailbox: &str) -> Result<Vec<u32>> {
        self.session.select(mailbox).await?;
        let uids = self.session.uid_search("ALL").await?;
        Ok(uids.into_iter().collect())
    }

    /// Fetch UIDs greater than `since_uid` for incremental sync.
    pub async fn fetch_new_uids(&mut self, mailbox: &str, since_uid: u32) -> Result<Vec<u32>> {
        self.session.select(mailbox).await?;
        let query = format!("UID {}:*", since_uid + 1);
        let uids = self.session.uid_search(&query).await?;
        // Filter out since_uid itself (IMAP may include it)
        Ok(uids.into_iter().filter(|&uid| uid > since_uid).collect())
    }

    /// Fetch full message bodies and flags for the given UIDs.
    /// The mailbox must already be selected (call fetch_uids or fetch_new_uids first).
    pub async fn fetch_full_messages(&mut self, uids: &[u32]) -> Result<Vec<FetchedMessage>> {
        if uids.is_empty() {
            return Ok(Vec::new());
        }

        let uid_set: String = uids
            .iter()
            .map(|u| u.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let fetch_data: Vec<_> = self
            .session
            .uid_fetch(&uid_set, "(UID FLAGS BODY.PEEK[])")
            .await?
            .try_collect()
            .await?;

        let mut result = Vec::with_capacity(fetch_data.len());

        for msg in &fetch_data {
            let Some(uid) = msg.uid else { continue };
            let Some(body_bytes) = msg.body() else {
                continue;
            };

            let is_read = msg
                .flags()
                .any(|f| matches!(f, async_imap::types::Flag::Seen));
            let is_starred = msg
                .flags()
                .any(|f| matches!(f, async_imap::types::Flag::Flagged));

            let parsed = mail_parser::MessageParser::default().parse(body_bytes);

            let (subject, from_name, from_email, to, date, body_text) = match parsed {
                Some(ref parsed_msg) => {
                    let subject = parsed_msg.subject().unwrap_or("").to_string();

                    let (from_name, from_email) = parsed_msg
                        .from()
                        .and_then(|f| f.first())
                        .map(|addr| {
                            (
                                addr.name().unwrap_or("").to_string(),
                                addr.address().unwrap_or("").to_string(),
                            )
                        })
                        .unwrap_or_default();

                    let to: Vec<String> = parsed_msg
                        .to()
                        .map(|list| {
                            list.iter()
                                .filter_map(|a| a.address().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();

                    let date = parsed_msg
                        .date()
                        .map(|d| date_to_utc_rfc3339(d))
                        .unwrap_or_default();

                    let body_text = parsed_msg.body_text(0).unwrap_or_default().to_string();

                    (subject, from_name, from_email, to, date, body_text)
                }
                None => {
                    tracing::warn!(uid, "failed to parse message body");
                    continue;
                }
            };

            result.push(FetchedMessage {
                uid,
                subject,
                from_name,
                from_email,
                to,
                date,
                body: body_text,
                is_read,
                is_starred,
            });
        }

        Ok(result)
    }

    /// Add flags to a message by UID. The mailbox must already be selected.
    pub async fn add_flags(&mut self, uid: u32, flags: &str) -> Result<()> {
        let uid_set = uid.to_string();
        let query = format!("+FLAGS ({flags})");
        let _: Vec<_> = self
            .session
            .uid_store(&uid_set, &query)
            .await?
            .try_collect()
            .await?;
        Ok(())
    }

    /// Remove flags from a message by UID. The mailbox must already be selected.
    pub async fn remove_flags(&mut self, uid: u32, flags: &str) -> Result<()> {
        let uid_set = uid.to_string();
        let query = format!("-FLAGS ({flags})");
        let _: Vec<_> = self
            .session
            .uid_store(&uid_set, &query)
            .await?
            .try_collect()
            .await?;
        Ok(())
    }

    /// Move a message from the currently selected mailbox to another mailbox.
    /// Uses COPY + STORE \Deleted + EXPUNGE for maximum compatibility.
    pub async fn move_message(&mut self, uid: u32, to_mailbox: &str) -> Result<()> {
        let uid_set = uid.to_string();
        self.session.uid_copy(&uid_set, to_mailbox).await?;
        let _: Vec<_> = self
            .session
            .uid_store(&uid_set, "+FLAGS (\\Deleted)")
            .await?
            .try_collect()
            .await?;
        let _: Vec<_> = self.session.expunge().await?.try_collect().await?;
        Ok(())
    }

    /// Select a mailbox for subsequent operations.
    pub async fn select(&mut self, mailbox: &str) -> Result<()> {
        self.session.select(mailbox).await?;
        Ok(())
    }

    /// Permanently delete a message by UID. The mailbox must already be selected.
    pub async fn delete_message(&mut self, uid: u32) -> Result<()> {
        let uid_set = uid.to_string();
        let _: Vec<_> = self
            .session
            .uid_store(&uid_set, "+FLAGS (\\Deleted)")
            .await?
            .try_collect()
            .await?;
        let _: Vec<_> = self.session.expunge().await?.try_collect().await?;
        Ok(())
    }

    /// Append a raw RFC 2822 message to the given mailbox.
    pub async fn append(
        &mut self,
        mailbox: &str,
        content: &[u8],
        flags: Option<&str>,
    ) -> Result<()> {
        self.session.append(mailbox, flags, None, content).await?;
        Ok(())
    }

    pub async fn logout(mut self) -> Result<()> {
        self.session.logout().await?;
        Ok(())
    }
}

fn tls_connector() -> TlsConnector {
    let root_store =
        rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    TlsConnector::from(Arc::new(config))
}

fn io_err(e: std::io::Error) -> Error {
    Error::Io(e)
}
