use std::collections::BTreeMap;
use std::sync::Mutex;

use chrono::{DateTime, Utc};

use super::*;
use crate::delivery::ProviderSendStatus;

#[tokio::test]
async fn sends_plain_text_email_message() {
    let client = Arc::new(RecordingEmailSmtpClient::success());
    let provider = EmailSmtpProvider::new_for_test(
        "email",
        "Agents Notifier <alerts@example.com>",
        &["Felix <felix@example.com>"],
        Some("Agents Notifier <reply@example.com>"),
        client.clone(),
    );

    let result = provider
        .send(&test_signal("Ready for review."))
        .await
        .expect("email send should succeed");

    assert_eq!(result.provider_id, "email");
    assert_eq!(result.provider_type, "email_smtp");
    assert_eq!(result.status, ProviderSendStatus::Sent);

    let message = client.single_message();
    assert!(message.contains("From:"));
    assert!(message.contains("alerts@example.com"));
    assert!(message.contains("To:"));
    assert!(message.contains("felix@example.com"));
    assert!(message.contains("Reply-To:"));
    assert!(message.contains("reply@example.com"));
    assert!(message.contains("Subject: Codex"));
    assert!(message.contains("Content-Type: text/plain"));
    assert!(message.contains("Ready for review."));
    assert!(message.contains("Time: "));
}

#[tokio::test]
async fn returns_retriable_error_for_transient_smtp_failure() {
    let client = Arc::new(RecordingEmailSmtpClient::failure(EmailSmtpSendError {
        kind: EmailSmtpErrorKind::ProviderRejected,
        message: "transient SMTP failure".to_string(),
        provider_code: Some("421".to_string()),
        retriable: true,
    }));
    let provider = EmailSmtpProvider::new_for_test(
        "email",
        "alerts@example.com",
        &["felix@example.com"],
        None,
        client,
    );

    let err = provider
        .send(&test_signal("Ready for review."))
        .await
        .expect_err("transient SMTP failure should fail");

    assert_eq!(err.kind, DeliveryErrorKind::ProviderRejected);
    assert_eq!(err.provider_code.as_deref(), Some("421"));
    assert!(err.retriable);
}

#[test]
fn validates_email_smtp_config_values() {
    let provider = EmailSmtpProvider::from_config(&ProviderConfig {
        id: "email".to_string(),
        provider_type: ProviderType::EmailSmtp,
        base_url: None,
        server: None,
        topic: None,
        url: None,
        url_env: None,
        secret: None,
        secret_env: None,
        app_token: None,
        app_token_env: None,
        user_key: None,
        user_key_env: None,
        device: None,
        sound: None,
        bot_token: None,
        bot_token_env: None,
        chat_id: None,
        access_token: None,
        access_token_env: None,
        phone_number_id: None,
        recipient_phone_number: None,
        host: Some("smtp.example.com".to_string()),
        port: Some(587),
        security: Some(EmailSmtpSecurity::Starttls),
        username: Some("alerts@example.com".to_string()),
        username_env: None,
        password: Some("smtp-password".to_string()),
        password_env: None,
        from: Some("Agents Notifier <alerts@example.com>".to_string()),
        to: Some(vec!["Felix <felix@example.com>".to_string()]),
        reply_to: Some("reply@example.com".to_string()),
        token: None,
        token_env: None,
        recipient_user_id: None,
        context_token: None,
        context_token_env: None,
        route_tag: None,
    })
    .expect("email_smtp provider config should be valid");

    assert_eq!(provider.id, "email");
}

#[test]
fn rejects_invalid_email_smtp_host_and_mailbox() {
    let err = parse_mailbox("email", "from", Some("not an email"))
        .expect_err("invalid mailbox should fail");
    assert!(err.to_string().contains("valid email mailbox"));

    let err =
        resolve_host("email", Some("https://smtp.example.com")).expect_err("host URL should fail");
    assert!(err.to_string().contains("hostname"));
}

#[derive(Debug)]
struct RecordingEmailSmtpClient {
    messages: Mutex<Vec<String>>,
    result: Result<(), EmailSmtpSendError>,
}

impl RecordingEmailSmtpClient {
    fn success() -> Self {
        Self {
            messages: Mutex::new(Vec::new()),
            result: Ok(()),
        }
    }

    fn failure(error: EmailSmtpSendError) -> Self {
        Self {
            messages: Mutex::new(Vec::new()),
            result: Err(error),
        }
    }

    fn single_message(&self) -> String {
        let messages = self.messages.lock().expect("messages lock should be valid");
        assert_eq!(messages.len(), 1);
        messages[0].clone()
    }
}

impl EmailSmtpClient for RecordingEmailSmtpClient {
    fn send<'a>(
        &'a self,
        message: Message,
    ) -> Pin<Box<dyn Future<Output = Result<(), EmailSmtpSendError>> + Send + 'a>> {
        let formatted =
            String::from_utf8(message.formatted()).expect("formatted test message should be UTF-8");
        self.messages
            .lock()
            .expect("messages lock should be valid")
            .push(formatted);
        let result = self.result.clone();

        Box::pin(async move { result })
    }
}

fn test_signal(body: &str) -> Signal {
    Signal::new_with_timestamp(
        "signal-1",
        "codex_cli",
        "codex_cli",
        "Codex",
        body,
        test_timestamp(),
        BTreeMap::new(),
    )
}

fn test_timestamp() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-05-10T10:31:43Z")
        .expect("timestamp should parse")
        .with_timezone(&Utc)
}
