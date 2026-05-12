use serde::Serialize;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub(crate) struct ProviderHttpSimulator {
    server: MockServer,
}

impl ProviderHttpSimulator {
    pub(crate) async fn start() -> Self {
        Self {
            server: MockServer::start().await,
        }
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{}", self.server.uri(), path)
    }

    pub(crate) async fn expect_post_json<T>(&self, path_value: &str, body: &T, status: u16)
    where
        T: Serialize,
    {
        Mock::given(method("POST"))
            .and(path(path_value))
            .and(body_json(body))
            .respond_with(ResponseTemplate::new(status))
            .mount(&self.server)
            .await;
    }

    pub(crate) async fn expect_post_status(&self, path_value: &str, status: u16) {
        Mock::given(method("POST"))
            .and(path(path_value))
            .respond_with(ResponseTemplate::new(status))
            .mount(&self.server)
            .await;
    }
}
