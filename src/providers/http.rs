use std::time::Duration;

pub(super) const PROVIDER_HTTP_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) fn provider_http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(PROVIDER_HTTP_TIMEOUT)
        .build()
        .map_err(anyhow::Error::from)
}
