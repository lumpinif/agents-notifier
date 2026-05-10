use anyhow::Context;
use reqwest::Url;

pub fn validate_slack_webhook_url(input: &str) -> anyhow::Result<String> {
    let url = parse_https_provider_url(input, "Slack webhook URL")?;
    let host = url.host_str().unwrap_or_default();
    if !matches!(host, "hooks.slack.com" | "hooks.slack-gov.com") {
        anyhow::bail!("Slack webhook URL must use `hooks.slack.com` or `hooks.slack-gov.com`");
    }

    let segments = path_segments(&url);
    let has_expected_path = segments.len() == 4
        && segments[0] == "services"
        && segments[1..].iter().all(|segment| !segment.is_empty());
    if !has_expected_path {
        anyhow::bail!("Slack webhook URL must look like `https://hooks.slack.com/services/...`");
    }

    Ok(url.to_string())
}

pub fn validate_discord_webhook_url(input: &str) -> anyhow::Result<String> {
    let url = parse_https_provider_url(input, "Discord webhook URL")?;
    let host = url.host_str().unwrap_or_default();
    if !matches!(host, "discord.com" | "discordapp.com") {
        anyhow::bail!("Discord webhook URL must use `discord.com` or `discordapp.com`");
    }

    let segments = path_segments(&url);
    let has_expected_path = segments.len() == 4
        && segments[0] == "api"
        && segments[1] == "webhooks"
        && segments[2].bytes().all(|byte| byte.is_ascii_digit())
        && !segments[3].is_empty();
    if !has_expected_path {
        anyhow::bail!(
            "Discord webhook URL must look like `https://discord.com/api/webhooks/{{id}}/{{token}}`"
        );
    }

    Ok(url.to_string())
}

pub fn validate_custom_webhook_url(input: &str) -> anyhow::Result<String> {
    let url = input.trim();
    if url.is_empty() {
        anyhow::bail!("webhook URL is required");
    }

    let parsed = Url::parse(url).map_err(|_| anyhow::anyhow!("webhook URL must be a valid URL"))?;
    let Some(host) = parsed.host_str() else {
        anyhow::bail!("webhook URL must include a host");
    };

    let scheme = parsed.scheme();
    let is_local_http = scheme == "http" && matches!(host, "localhost" | "127.0.0.1" | "::1");
    if scheme != "https" && !is_local_http {
        anyhow::bail!("webhook URL must use HTTPS, except localhost test URLs");
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        anyhow::bail!("webhook URL must not include username or password");
    }

    Ok(parsed.to_string())
}

pub fn host_label(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "configured".to_string())
}

fn parse_https_provider_url(input: &str, label: &'static str) -> anyhow::Result<Url> {
    let url = input.trim();
    if url.is_empty() {
        anyhow::bail!("{label} is required");
    }

    let parsed = Url::parse(url).with_context(|| format!("{label} must be a valid URL"))?;
    if parsed.scheme() != "https" {
        anyhow::bail!("{label} must use HTTPS");
    }
    if parsed.host_str().is_none() {
        anyhow::bail!("{label} must include a host");
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        anyhow::bail!("{label} must not include username or password");
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        anyhow::bail!("{label} must not include a query string or fragment");
    }

    Ok(parsed)
}

fn path_segments(url: &Url) -> Vec<&str> {
    url.path_segments()
        .map(|segments| segments.collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_official_slack_webhook_urls() {
        let slack_url = slack_test_url("hooks.slack.com");
        let gov_slack_url = slack_test_url("hooks.slack-gov.com");

        assert_eq!(
            validate_slack_webhook_url(&slack_url).expect("Slack webhook URL should be valid"),
            slack_url
        );

        assert_eq!(
            validate_slack_webhook_url(&gov_slack_url)
                .expect("GovSlack webhook URL should be valid"),
            gov_slack_url
        );
    }

    #[test]
    fn rejects_non_slack_webhook_urls() {
        let err = validate_slack_webhook_url("https://example.com/services/T/B/token")
            .expect_err("wrong Slack host should fail");

        assert!(err.to_string().contains("hooks.slack.com"));
    }

    #[test]
    fn accepts_official_discord_webhook_urls() {
        assert_eq!(
            validate_discord_webhook_url(
                "https://discord.com/api/webhooks/123456789012345678/token"
            )
            .expect("Discord webhook URL should be valid"),
            "https://discord.com/api/webhooks/123456789012345678/token"
        );
    }

    #[test]
    fn rejects_non_discord_webhook_urls() {
        let err = validate_discord_webhook_url("https://example.com/api/webhooks/123/token")
            .expect_err("wrong Discord host should fail");

        assert!(err.to_string().contains("discord.com"));
    }

    #[test]
    fn accepts_https_and_local_custom_webhook_urls() {
        assert_eq!(
            validate_custom_webhook_url("https://example.com/agents-notifier")
                .expect("HTTPS webhook should be valid"),
            "https://example.com/agents-notifier"
        );
        assert_eq!(
            validate_custom_webhook_url("http://127.0.0.1:8080/hook")
                .expect("local HTTP webhook should be valid"),
            "http://127.0.0.1:8080/hook"
        );
    }

    fn slack_test_url(host: &str) -> String {
        format!(
            "https://{}/services/{}/{}/{}",
            host, "T00000000", "B00000000", "test-token"
        )
    }
}
