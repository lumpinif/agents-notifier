use super::*;

pub async fn fetch_wechat_qr_code(
    base_url: &str,
    route_tag: Option<&str>,
    bot_type: &str,
) -> anyhow::Result<WechatQrCode> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(40))
        .build()
        .context("failed to build WeChat setup HTTP client")?;
    let mut url = wechat_url(base_url, &["ilink", "bot", "get_bot_qrcode"])?;
    url.query_pairs_mut().append_pair("bot_type", bot_type);

    let mut request = client.get(url);
    if let Some(route_tag) = route_tag.filter(|value| !value.trim().is_empty()) {
        request = request.header("SKRouteTag", route_tag);
    }

    let response = request
        .send()
        .await
        .context("failed to request WeChat QR code")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read WeChat QR code response")?;
    if !status.is_success() {
        anyhow::bail!(
            "WeChat QR code request returned HTTP {}: {}",
            status.as_u16(),
            short_response_body(&body)
        );
    }

    let payload: WechatQrCodeResponse =
        serde_json::from_str(&body).context("failed to parse WeChat QR code response")?;
    let qr_key = payload.qrcode.trim();
    let qr_content = payload.qrcode_img_content.trim();
    if qr_key.is_empty() {
        anyhow::bail!("WeChat QR code response did not include `qrcode`");
    }
    if qr_content.is_empty() {
        anyhow::bail!("WeChat QR code response did not include `qrcode_img_content`");
    }

    Ok(WechatQrCode {
        qr_key: qr_key.to_string(),
        qr_content: qr_content.to_string(),
    })
}

pub async fn poll_wechat_qr_status(
    base_url: &str,
    qr_key: &str,
    route_tag: Option<&str>,
) -> anyhow::Result<WechatQrStatus> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(40))
        .build()
        .context("failed to build WeChat setup HTTP client")?;
    let mut url = wechat_url(base_url, &["ilink", "bot", "get_qrcode_status"])?;
    url.query_pairs_mut().append_pair("qrcode", qr_key);

    let mut request = client.get(url).header("iLink-App-ClientVersion", "1");
    if let Some(route_tag) = route_tag.filter(|value| !value.trim().is_empty()) {
        request = request.header("SKRouteTag", route_tag);
    }

    let response = request
        .send()
        .await
        .context("failed to request WeChat QR status")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read WeChat QR status response")?;
    if !status.is_success() {
        anyhow::bail!(
            "WeChat QR status request returned HTTP {}: {}",
            status.as_u16(),
            short_response_body(&body)
        );
    }

    let payload: WechatQrStatusResponse =
        serde_json::from_str(&body).context("failed to parse WeChat QR status response")?;

    Ok(WechatQrStatus {
        status: payload.status.trim().to_string(),
        token: present_str(payload.bot_token),
        account_id: present_str(payload.ilink_bot_id),
        base_url: present_str(payload.baseurl).map(|value| value.trim_end_matches('/').to_string()),
        scanned_user_id: present_str(payload.ilink_user_id),
    })
}

pub async fn verify_wechat_token(
    base_url: &str,
    token: &str,
    route_tag: Option<&str>,
) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .context("failed to build WeChat setup HTTP client")?;
    let url = wechat_url(base_url, &["ilink", "bot", "getupdates"])?;
    let body = json!({
        "get_updates_buf": "",
        "base_info": {
            "channel_version": WECHAT_SETUP_CHANNEL_VERSION
        }
    });

    let mut request = client
        .post(url)
        .bearer_auth(token)
        .header("AuthorizationType", "ilink_bot_token")
        .header("X-WECHAT-UIN", random_wechat_uin())
        .json(&body);
    if let Some(route_tag) = route_tag.filter(|value| !value.trim().is_empty()) {
        request = request.header("SKRouteTag", route_tag);
    }

    let response = request
        .send()
        .await
        .context("failed to verify WeChat iLink token")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read WeChat token verification response")?;
    if !status.is_success() {
        anyhow::bail!(
            "WeChat token verification returned HTTP {}: {}",
            status.as_u16(),
            short_response_body(&body)
        );
    }
    let _: serde_json::Value =
        serde_json::from_str(&body).context("WeChat token verification returned invalid JSON")?;

    Ok(())
}

pub async fn poll_wechat_recipient_link(
    base_url: &str,
    token: &str,
    route_tag: Option<&str>,
    expected_user_id: Option<&str>,
    timeout: Duration,
) -> anyhow::Result<WechatRecipientLink> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(40))
        .build()
        .context("failed to build WeChat setup HTTP client")?;
    let url = wechat_url(base_url, &["ilink", "bot", "getupdates"])?;
    let deadline = Instant::now() + timeout;
    let mut get_updates_buf = String::new();

    while Instant::now() < deadline {
        let body = json!({
            "get_updates_buf": get_updates_buf,
            "base_info": {
                "channel_version": WECHAT_SETUP_CHANNEL_VERSION
            }
        });
        let mut request = client
            .post(url.clone())
            .bearer_auth(token)
            .header("AuthorizationType", "ilink_bot_token")
            .header("X-WECHAT-UIN", random_wechat_uin())
            .json(&body);
        if let Some(route_tag) = route_tag.filter(|value| !value.trim().is_empty()) {
            request = request.header("SKRouteTag", route_tag);
        }

        let response = request
            .send()
            .await
            .context("failed to poll WeChat recipient link message")?;
        let status = response.status();
        let response_body = response
            .text()
            .await
            .context("failed to read WeChat recipient link response")?;
        if !status.is_success() {
            anyhow::bail!(
                "WeChat recipient link poll returned HTTP {}: {}",
                status.as_u16(),
                short_response_body(&response_body)
            );
        }

        let payload: WechatGetUpdatesResponse = serde_json::from_str(&response_body)
            .context("failed to parse WeChat recipient link response")?;
        if payload.ret != 0 || payload.errcode != 0 {
            anyhow::bail!(
                "WeChat recipient link poll returned ret={} errcode={}: {}",
                payload.ret,
                payload.errcode,
                payload.errmsg
            );
        }
        get_updates_buf = payload.get_updates_buf.unwrap_or_default();

        for message in payload.msgs {
            let from_user_id = message.from_user_id.trim();
            let context_token = message.context_token.trim();
            if message.message_type == Some(2) {
                continue;
            }
            if from_user_id.is_empty() || context_token.is_empty() {
                continue;
            }
            if expected_user_id.is_some_and(|expected| expected != from_user_id) {
                continue;
            }

            return Ok(WechatRecipientLink {
                recipient_user_id: from_user_id.to_string(),
                context_token: context_token.to_string(),
            });
        }

        sleep(Duration::from_secs(1)).await;
    }

    anyhow::bail!("timed out waiting for a WeChat message with context_token")
}

#[derive(Debug, Deserialize)]
struct WechatQrCodeResponse {
    qrcode: String,
    qrcode_img_content: String,
}

#[derive(Debug, Deserialize)]
struct WechatQrStatusResponse {
    #[serde(default)]
    status: String,
    #[serde(default)]
    bot_token: String,
    #[serde(default)]
    ilink_bot_id: String,
    #[serde(default)]
    baseurl: String,
    #[serde(default)]
    ilink_user_id: String,
}

#[derive(Debug, Deserialize)]
struct WechatGetUpdatesResponse {
    #[serde(default)]
    ret: i64,
    #[serde(default)]
    errcode: i64,
    #[serde(default)]
    errmsg: String,
    #[serde(default)]
    msgs: Vec<WechatUpdateMessage>,
    #[serde(default)]
    get_updates_buf: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WechatUpdateMessage {
    #[serde(default)]
    from_user_id: String,
    #[serde(default)]
    context_token: String,
    #[serde(default)]
    message_type: Option<i64>,
}

fn wechat_url(base_url: &str, path: &[&str]) -> anyhow::Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(&resolve_wechat_base_url(base_url)?)?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("WeChat iLink base URL cannot be a base URL"))?
        .extend(path);
    Ok(url)
}

fn short_response_body(body: &str) -> String {
    let body = body.trim();
    if body.chars().count() <= 256 {
        return body.to_string();
    }

    format!("{}...", body.chars().take(256).collect::<String>())
}

fn present_str(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

fn random_wechat_uin() -> String {
    let bytes = Uuid::new_v4().into_bytes();
    let value = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    BASE64_STANDARD.encode(value.to_string())
}
