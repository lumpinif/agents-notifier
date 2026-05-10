use anyhow::{Context, anyhow};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, warn};

pub const LOCAL_OPEN_BRIDGE_BASE_URL: &str = "http://127.0.0.1:17674";
const LISTEN_ADDRESS: &str = "127.0.0.1:17674";
const CODEX_THREAD_PREFIX: &str = "/open/codex/thread/";

pub fn codex_thread_bridge_url(session_id: &str) -> Option<String> {
    let session_id = session_id.trim();
    if !is_safe_path_segment(session_id) {
        return None;
    }

    Some(format!(
        "{LOCAL_OPEN_BRIDGE_BASE_URL}{CODEX_THREAD_PREFIX}{session_id}"
    ))
}

pub async fn serve() -> anyhow::Result<()> {
    let listener = TcpListener::bind(LISTEN_ADDRESS)
        .await
        .with_context(|| format!("failed to bind local open bridge at `{LISTEN_ADDRESS}`"))?;

    info!(address = LISTEN_ADDRESS, event = "link_bridge.started",);

    loop {
        let (stream, _) = listener
            .accept()
            .await
            .context("failed to accept local open bridge connection")?;
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream).await {
                warn!(
                    error = %error,
                    event = "link_bridge.client.failed",
                );
            }
        });
    }
}

async fn handle_connection(mut stream: TcpStream) -> anyhow::Result<()> {
    let mut buffer = [0; 2048];
    let bytes_read = stream
        .read(&mut buffer)
        .await
        .context("failed to read local open bridge request")?;
    if bytes_read == 0 {
        return Ok(());
    }

    let request = std::str::from_utf8(&buffer[..bytes_read])
        .context("local open bridge request was not valid UTF-8")?;
    let path = request_path(request)?;
    let response = response_for_path(path);

    stream
        .write_all(response.as_bytes())
        .await
        .context("failed to write local open bridge response")?;
    Ok(())
}

fn request_path(request: &str) -> anyhow::Result<&str> {
    let request_line = request
        .lines()
        .next()
        .ok_or_else(|| anyhow!("local open bridge request was empty"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();

    if method != "GET" {
        return Err(anyhow!("local open bridge only accepts GET requests"));
    }
    if path.is_empty() {
        return Err(anyhow!("local open bridge request path was empty"));
    }

    Ok(path)
}

fn response_for_path(path: &str) -> String {
    match codex_thread_deep_link_from_path(path) {
        Some(deep_link) => redirect_response(&deep_link),
        None => not_found_response(),
    }
}

fn codex_thread_deep_link_from_path(path: &str) -> Option<String> {
    let session_id = path.strip_prefix(CODEX_THREAD_PREFIX)?;
    if !is_safe_path_segment(session_id) {
        return None;
    }

    Some(format!("codex://threads/{session_id}"))
}

fn is_safe_path_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn redirect_response(location: &str) -> String {
    format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
}

fn not_found_response() -> String {
    "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: 9\r\nConnection: close\r\n\r\nNot Found"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_codex_thread_bridge_url_for_safe_session_id() {
        assert_eq!(
            codex_thread_bridge_url("019e1049-2d6d-7de2-bcdf-f47346930b71"),
            Some(
                "http://127.0.0.1:17674/open/codex/thread/019e1049-2d6d-7de2-bcdf-f47346930b71"
                    .to_string()
            )
        );
    }

    #[test]
    fn rejects_unsafe_codex_thread_bridge_ids() {
        assert_eq!(codex_thread_bridge_url(""), None);
        assert_eq!(codex_thread_bridge_url("../Library"), None);
        assert_eq!(
            codex_thread_bridge_url("thread?url=https://example.com"),
            None
        );
    }

    #[test]
    fn redirects_only_known_codex_thread_paths() {
        assert_eq!(
            response_for_path("/open/codex/thread/session-1"),
            "HTTP/1.1 302 Found\r\nLocation: codex://threads/session-1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        assert!(response_for_path("/open/other").starts_with("HTTP/1.1 404 Not Found"));
    }
}
