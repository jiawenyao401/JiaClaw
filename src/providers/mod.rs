pub mod brokerrouter;
pub mod openai_compatible;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProviderType {
    Brokerrouter,
    #[deprecated(note = "Use Brokerrouter provider for production. This is a temporary escape hatch.")]
    OpenAiCompatible,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[async_trait]
pub trait Provider: Send + Sync {
    async fn chat_completion(&self, request: ChatRequest) -> Result<ChatResponse>;
}

#[allow(deprecated)]
pub fn create_provider(
    provider_type: &str,
    base_url: String,
    api_key: String,
) -> Result<Box<dyn Provider>> {
    match provider_type {
        "brokerrouter" => Ok(Box::new(brokerrouter::BrokerrouterProvider::new(
            base_url, api_key,
        ))),
        "openai_compatible" => Ok(Box::new(
            openai_compatible::OpenAiCompatibleProvider::new(base_url, api_key),
        )),
        _ => Err(crate::Error::ConfigError(format!(
            "Unknown provider type: {}. Use 'brokerrouter' (recommended) or 'openai_compatible' (deprecated)",
            provider_type
        ))),
    }
}
