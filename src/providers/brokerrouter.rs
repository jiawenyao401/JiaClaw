use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::providers::{ChatRequest, ChatResponse, Provider};

/// Brokerrouter provider - the recommended production provider for JiaClaw.
///
/// Connects to Brokerrouter (https://github.com/StateKnot/Brokerrouter),
/// a private Rust AI gateway implementing the OpenAI-compatible chat completions API.
///
/// # Features
/// - Non-streaming chat completions via `POST /v1/chat/completions`
/// - Virtual-key Bearer authentication
/// - Automatic idempotency key generation per request
/// - Request ID tracking via response headers
///
/// # Limitations (tracked in StateKnot/Brokerrouter issues)
/// - Streaming not yet supported (issue #29)
/// - Consumer guide pending (issue #28)
/// - Tool calling roundtrip certification pending (issue #31)
///
/// # References
/// - [Brokerrouter Repository](https://github.com/StateKnot/Brokerrouter)
/// - [Issue #28: Consumer guide](https://github.com/StateKnot/Brokerrouter/issues/28)
/// - [Issue #29: Streaming support](https://github.com/StateKnot/Brokerrouter/issues/29)
/// - [Issue #30: Personal bootstrap](https://github.com/StateKnot/Brokerrouter/issues/30)
/// - [Issue #31: Tool roundtrip cert](https://github.com/StateKnot/Brokerrouter/issues/31)
pub struct BrokerrouterProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

#[derive(Debug, Serialize)]
struct BrokerrouterRequest {
    model: String,
    messages: Vec<BrokerrouterMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct BrokerrouterMessage {
    role: String,
    content: String,
}

impl BrokerrouterProvider {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
        }
    }

    fn generate_idempotency_key() -> Result<String> {
        let key = format!("jiaclaw-{}", Uuid::new_v4());

        if key.len() > 200 {
            return Err(Error::InvalidIdempotencyKey(
                "Generated key exceeds 200 characters".to_string(),
            ));
        }

        if !key.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
            return Err(Error::InvalidIdempotencyKey(
                "Key contains non-printable ASCII characters".to_string(),
            ));
        }

        Ok(key)
    }

    fn map_error(&self, status: StatusCode, body: &str) -> Error {
        let message = serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|v| {
                v.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| body.to_string());

        Error::ApiError {
            status: status.as_u16(),
            message,
        }
    }
}

#[async_trait]
impl Provider for BrokerrouterProvider {
    async fn chat_completion(&self, request: ChatRequest) -> Result<ChatResponse> {
        let idempotency_key = Self::generate_idempotency_key()?;

        let broker_request = BrokerrouterRequest {
            model: request.model,
            messages: request
                .messages
                .into_iter()
                .map(|m| BrokerrouterMessage {
                    role: m.role,
                    content: m.content,
                })
                .collect(),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: false,
        };

        let url = format!("{}/v1/chat/completions", self.base_url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Idempotency-Key", idempotency_key)
            .header("Content-Type", "application/json")
            .json(&broker_request)
            .send()
            .await?;

        let status = response.status();
        let headers = response.headers().clone();

        let request_id = headers
            .get("x-request-id")
            .or_else(|| headers.get("x-brokerrouter-request-id"))
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(self.map_error(status, &body));
        }

        let body = response.text().await?;
        let mut chat_response: ChatResponse = serde_json::from_str(&body)?;

        if let Some(rid) = request_id {
            chat_response.request_id = Some(rid);
        }

        Ok(chat_response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ChatMessage;

    #[test]
    fn test_generate_idempotency_key() {
        let key = BrokerrouterProvider::generate_idempotency_key().unwrap();
        assert!(key.len() <= 200);
        assert!(key.starts_with("jiaclaw-"));
        assert!(key.chars().all(|c| c.is_ascii_graphic() || c == ' '));
    }

    #[test]
    fn test_idempotency_key_uniqueness() {
        let key1 = BrokerrouterProvider::generate_idempotency_key().unwrap();
        let key2 = BrokerrouterProvider::generate_idempotency_key().unwrap();
        assert_ne!(key1, key2);
    }

    #[tokio::test]
    async fn test_chat_completion_with_mock() {
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        let mock_response = serde_json::json!({
            "id": "chatcmpl-123",
            "model": "claude-3-5-sonnet-20241022",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I help you?"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 20,
                "total_tokens": 30
            }
        });

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("Authorization", "Bearer test-key"))
            .and(header("Content-Type", "application/json"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(&mock_response)
                    .insert_header("x-request-id", "req-123")
                    .insert_header("x-brokerrouter-request-id", "brr-456"),
            )
            .mount(&mock_server)
            .await;

        let provider = BrokerrouterProvider::new(mock_server.uri(), "test-key".to_string());

        let request = ChatRequest {
            model: "claude-3-5-sonnet-20241022".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            temperature: Some(0.7),
            max_tokens: Some(100),
        };

        let response = provider.chat_completion(request).await.unwrap();

        assert_eq!(response.id, "chatcmpl-123");
        assert_eq!(response.model, "claude-3-5-sonnet-20241022");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].message.role, "assistant");
        assert_eq!(
            response.choices[0].message.content,
            "Hello! How can I help you?"
        );
        assert!(response.request_id.is_some());
    }

    #[tokio::test]
    async fn test_error_handling() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        let error_response = serde_json::json!({
            "error": {
                "message": "Invalid API key",
                "type": "invalid_request_error"
            }
        });

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_json(&error_response))
            .mount(&mock_server)
            .await;

        let provider = BrokerrouterProvider::new(mock_server.uri(), "bad-key".to_string());

        let request = ChatRequest {
            model: "claude-3-5-sonnet-20241022".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            temperature: None,
            max_tokens: None,
        };

        let result = provider.chat_completion(request).await;
        assert!(result.is_err());

        if let Err(Error::ApiError { status, message }) = result {
            assert_eq!(status, 401);
            assert!(message.contains("Invalid API key"));
        } else {
            panic!("Expected ApiError");
        }
    }

    #[tokio::test]
    async fn test_non_streaming_request() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "test",
                "model": "test-model",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "test"},
                    "finish_reason": "stop"
                }]
            })))
            .mount(&mock_server)
            .await;

        let provider = BrokerrouterProvider::new(mock_server.uri(), "test-key".to_string());

        let request = ChatRequest {
            model: "test-model".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "test".to_string(),
            }],
            temperature: None,
            max_tokens: None,
        };

        let result = provider.chat_completion(request).await;
        assert!(result.is_ok());
        
        let response = result.unwrap();
        assert_eq!(response.id, "test");
        assert_eq!(response.model, "test-model");
    }
}
