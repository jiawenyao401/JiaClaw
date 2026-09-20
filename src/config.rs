use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub provider: ProviderConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(rename = "type")]
    pub provider_type: String,
    pub base_url: String,
    pub api_key: String,
    #[serde(default)]
    pub model: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: ProviderConfig {
                provider_type: "brokerrouter".to_string(),
                base_url: "https://api.brokerrouter.dev".to_string(),
                api_key: String::new(),
                model: Some("claude-3-5-sonnet-20241022".to_string()),
            },
        }
    }
}
