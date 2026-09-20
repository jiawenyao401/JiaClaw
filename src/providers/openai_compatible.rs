use async_trait::async_trait;
use reqwest::Client;

use crate::error::Result;
use crate::providers::{ChatRequest, ChatResponse, Provider};

/// OpenAI-compatible provider stub.
///
/// # Deprecation Notice
/// This provider is a temporary escape hatch for development and testing.
/// For production use, please use the `BrokerrouterProvider` instead.
///
/// The Brokerrouter provider offers:
/// - Proper virtual-key authentication
/// - Idempotency guarantees
/// - Request tracking
/// - Production-grade error handling
///
/// This stub will be removed in a future release.
#[deprecated(
    note = "Use BrokerrouterProvider for production. This is a temporary escape hatch."
)]
pub struct OpenAiCompatibleProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

#[allow(deprecated)]
impl OpenAiCompatibleProvider {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
        }
    }
}

#[async_trait]
#[allow(deprecated)]
impl Provider for OpenAiCompatibleProvider {
    #[allow(deprecated)]
    async fn chat_completion(&self, request: ChatRequest) -> Result<ChatResponse> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        let chat_response = response.json::<ChatResponse>().await?;
        Ok(chat_response)
    }
}
