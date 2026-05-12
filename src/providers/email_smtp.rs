use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow};
use chrono::{DateTime, Local, Utc};
use lettre::message::{Mailbox, header::ContentType};
use lettre::transport::smtp::Error as SmtpError;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::{EmailSmtpSecurity, ProviderConfig, ProviderType};
use crate::delivery::{DeliveryError, DeliveryErrorContext, DeliveryErrorKind, ProviderSendResult};
use crate::providers::formatting::format_signal_body;
use crate::router::{Provider, ProviderFuture};
use crate::signal::Signal;

const SMTP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct EmailSmtpProvider {
    id: String,
    from: Mailbox,
    to: Vec<Mailbox>,
    reply_to: Option<Mailbox>,
    client: Arc<dyn EmailSmtpClient>,
}

impl EmailSmtpProvider {
    pub fn from_config(config: &ProviderConfig) -> anyhow::Result<Self> {
        if config.provider_type != ProviderType::EmailSmtp {
            return Err(anyhow!(
                "provider `{}` is not an email_smtp provider",
                config.id
            ));
        }

        let host = resolve_host(&config.id, config.host.as_deref())?;
        let port = validate_port(&config.id, config.port)?;
        let security = config
            .security
            .ok_or_else(|| anyhow!("email_smtp provider `{}` requires `security`", config.id))?;
        let username = resolve_optional_secret_value(
            config,
            "username",
            config.username.as_deref(),
            "username_env",
            config.username_env.as_deref(),
        )?;
        let password = resolve_optional_secret_value(
            config,
            "password",
            config.password.as_deref(),
            "password_env",
            config.password_env.as_deref(),
        )?;
        validate_credentials(&config.id, username.as_ref(), password.as_ref())?;

        let from = parse_mailbox(&config.id, "from", config.from.as_deref())?;
        let to = parse_recipients(&config.id, config.to.as_deref())?;
        let reply_to = config
            .reply_to
            .as_deref()
            .map(|value| parse_mailbox(&config.id, "reply_to", Some(value)))
            .transpose()?;

        let transport = build_smtp_transport(&host, port, security, username, password)?;

        Ok(Self {
            id: config.id.clone(),
            from,
            to,
            reply_to,
            client: Arc::new(LettreEmailSmtpClient { transport }),
        })
    }

    #[cfg(test)]
    fn new_for_test(
        id: &str,
        from: &str,
        to: &[&str],
        reply_to: Option<&str>,
        client: Arc<dyn EmailSmtpClient>,
    ) -> Self {
        Self {
            id: id.to_string(),
            from: from.parse().expect("test from mailbox should be valid"),
            to: to
                .iter()
                .map(|recipient| recipient.parse().expect("test recipient should be valid"))
                .collect(),
            reply_to: reply_to.map(|mailbox| {
                mailbox
                    .parse()
                    .expect("test reply_to mailbox should be valid")
            }),
            client,
        }
    }
}

impl Provider for EmailSmtpProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn provider_type(&self) -> &str {
        ProviderType::EmailSmtp.as_str()
    }

    fn send<'a>(&'a self, signal: &'a Signal) -> ProviderFuture<'a> {
        Box::pin(async move {
            let provider_type = ProviderType::EmailSmtp.as_str();
            let message = build_email_message(signal, &self.from, &self.to, self.reply_to.as_ref())
                .map_err(|error| {
                    DeliveryError::new(
                        DeliveryErrorKind::Internal,
                        DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                        format!("email_smtp provider `{}` failed to build message", self.id),
                    )
                    .with_source(error)
                })?;

            self.client.send(message).await.map_err(|error| {
                let mut delivery_error = DeliveryError::new(
                    error.delivery_error_kind(),
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format!("email_smtp provider `{}` send failed", self.id),
                )
                .with_retriable(error.retriable)
                .with_source(anyhow!(error.message));

                if let Some(provider_code) = error.provider_code {
                    delivery_error = delivery_error.with_provider_code(provider_code);
                }

                delivery_error
            })?;

            Ok(ProviderSendResult::sent(&self.id, provider_type, signal))
        })
    }
}

trait EmailSmtpClient: std::fmt::Debug + Send + Sync {
    fn send<'a>(
        &'a self,
        message: Message,
    ) -> Pin<Box<dyn Future<Output = Result<(), EmailSmtpSendError>> + Send + 'a>>;
}

#[derive(Debug)]
struct LettreEmailSmtpClient {
    transport: AsyncSmtpTransport<Tokio1Executor>,
}

impl EmailSmtpClient for LettreEmailSmtpClient {
    fn send<'a>(
        &'a self,
        message: Message,
    ) -> Pin<Box<dyn Future<Output = Result<(), EmailSmtpSendError>> + Send + 'a>> {
        Box::pin(async move {
            self.transport
                .send(message)
                .await
                .map(|_| ())
                .map_err(EmailSmtpSendError::from_smtp_error)
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmailSmtpErrorKind {
    Timeout,
    ProviderRejected,
    Network,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EmailSmtpSendError {
    kind: EmailSmtpErrorKind,
    message: String,
    provider_code: Option<String>,
    retriable: bool,
}

impl EmailSmtpSendError {
    fn from_smtp_error(error: SmtpError) -> Self {
        let kind = if error.is_timeout() {
            EmailSmtpErrorKind::Timeout
        } else if error.is_response() {
            EmailSmtpErrorKind::ProviderRejected
        } else {
            EmailSmtpErrorKind::Network
        };
        let retriable = error.is_timeout()
            || error.is_transient()
            || (!error.is_permanent() && !error.is_response());
        let provider_code = error.status().map(|status| status.to_string());

        Self {
            kind,
            message: error.to_string(),
            provider_code,
            retriable,
        }
    }

    fn delivery_error_kind(&self) -> DeliveryErrorKind {
        match self.kind {
            EmailSmtpErrorKind::Timeout => DeliveryErrorKind::Timeout,
            EmailSmtpErrorKind::ProviderRejected => DeliveryErrorKind::ProviderRejected,
            EmailSmtpErrorKind::Network => DeliveryErrorKind::Network,
        }
    }
}

fn build_email_message(
    signal: &Signal,
    from: &Mailbox,
    to: &[Mailbox],
    reply_to: Option<&Mailbox>,
) -> anyhow::Result<Message> {
    let body = format_email_body(signal);
    let mut builder = Message::builder()
        .from(from.clone())
        .subject(signal.title().to_string())
        .header(ContentType::TEXT_PLAIN);
    for recipient in to {
        builder = builder.to(recipient.clone());
    }
    if let Some(reply_to) = reply_to {
        builder = builder.reply_to(reply_to.clone());
    }

    builder.body(body).map_err(anyhow::Error::from)
}

fn build_smtp_transport(
    host: &str,
    port: u16,
    security: EmailSmtpSecurity,
    username: Option<String>,
    password: Option<String>,
) -> anyhow::Result<AsyncSmtpTransport<Tokio1Executor>> {
    let builder = match security {
        EmailSmtpSecurity::Starttls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host),
        EmailSmtpSecurity::ImplicitTls => AsyncSmtpTransport::<Tokio1Executor>::relay(host),
    }
    .with_context(|| {
        format!(
            "failed to create SMTP transport for `{host}` using `{}`",
            security.as_str()
        )
    })?
    .port(port)
    .timeout(Some(SMTP_TIMEOUT));

    let builder = match (username, password) {
        (Some(username), Some(password)) => {
            builder.credentials(Credentials::new(username, password))
        }
        (None, None) => builder,
        _ => unreachable!("email_smtp credentials were validated before transport creation"),
    };

    Ok(builder.build())
}

fn resolve_host(provider_id: &str, host: Option<&str>) -> anyhow::Result<String> {
    let Some(host) = present(host) else {
        return Err(anyhow!(
            "email_smtp provider `{provider_id}` requires `host`"
        ));
    };
    if host.contains("://")
        || host.contains('/')
        || host.contains('@')
        || host.bytes().any(|byte| byte.is_ascii_whitespace())
    {
        return Err(anyhow!(
            "email_smtp provider `{provider_id}` `host` must be a hostname, not a URL"
        ));
    }

    Ok(host.to_string())
}

fn validate_port(provider_id: &str, port: Option<u16>) -> anyhow::Result<u16> {
    let Some(port) = port else {
        return Err(anyhow!(
            "email_smtp provider `{provider_id}` requires `port`"
        ));
    };
    if port == 0 {
        return Err(anyhow!(
            "email_smtp provider `{provider_id}` `port` must be greater than 0"
        ));
    }

    Ok(port)
}

fn resolve_optional_secret_value(
    config: &ProviderConfig,
    value_field: &'static str,
    value: Option<&str>,
    env_field: &'static str,
    env_name: Option<&str>,
) -> anyhow::Result<Option<String>> {
    match (present(value), present(env_name)) {
        (Some(value), None) => Ok(Some(value.to_string())),
        (None, Some(env_name)) => std::env::var(env_name).map(Some).with_context(|| {
            format!(
                "email_smtp provider `{}` env var `{}` from `{}` is not set",
                config.id, env_name, env_field
            )
        }),
        (None, None) => Ok(None),
        (Some(_), Some(_)) => Err(anyhow!(
            "email_smtp provider `{}` must set at most one of `{}` or `{}`",
            config.id,
            value_field,
            env_field
        )),
    }
}

fn validate_credentials(
    provider_id: &str,
    username: Option<&String>,
    password: Option<&String>,
) -> anyhow::Result<()> {
    match (username, password) {
        (Some(username), Some(password)) => {
            if username.trim().is_empty() {
                return Err(anyhow!(
                    "email_smtp provider `{provider_id}` `username` must be non-empty"
                ));
            }
            if password.trim().is_empty() {
                return Err(anyhow!(
                    "email_smtp provider `{provider_id}` `password` must be non-empty"
                ));
            }
            Ok(())
        }
        (None, None) => Ok(()),
        _ => Err(anyhow!(
            "email_smtp provider `{provider_id}` must set both username and password, or neither"
        )),
    }
}

fn parse_mailbox(
    provider_id: &str,
    field: &'static str,
    value: Option<&str>,
) -> anyhow::Result<Mailbox> {
    let Some(value) = present(value) else {
        return Err(anyhow!(
            "email_smtp provider `{provider_id}` requires `{field}`"
        ));
    };

    value.parse().with_context(|| {
        format!("email_smtp provider `{provider_id}` `{field}` must be a valid email mailbox")
    })
}

fn parse_recipients(
    provider_id: &str,
    recipients: Option<&[String]>,
) -> anyhow::Result<Vec<Mailbox>> {
    let Some(recipients) = recipients.filter(|recipients| !recipients.is_empty()) else {
        return Err(anyhow!("email_smtp provider `{provider_id}` requires `to`"));
    };

    recipients
        .iter()
        .enumerate()
        .map(|(index, recipient)| {
            parse_mailbox(provider_id, "to", Some(recipient)).with_context(|| {
                format!("email_smtp provider `{provider_id}` `to[{index}]` is invalid")
            })
        })
        .collect()
}

fn format_email_body(signal: &Signal) -> String {
    format_signal_body(signal, &format_local_timestamp(signal.timestamp))
}

fn format_local_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S %:z")
        .to_string()
}

fn present(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests;
