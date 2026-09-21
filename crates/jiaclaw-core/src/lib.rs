// Copyright 2026 JiaClaw contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `JiaClaw` 核心领域类型和契约
//!
//! 本模块定义了 `JiaClaw` 个人智能体运行时的核心类型。

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 外部依赖
extern crate dirs;

/// `JiaClaw` 错误类型
#[derive(Debug, Error)]
pub enum JiaClawError {
    /// 配置错误
    #[error("配置错误: {0}")]
    Configuration(String),

    /// `StateKnot` 集成错误
    #[error("StateKnot 集成错误: {0}")]
    StateKnotIntegration(String),

    /// 无效请求
    #[error("无效请求: {0}")]
    InvalidRequest(String),

    /// 工具执行错误
    #[error("工具执行错误: {0}")]
    ToolExecution(String),
}

/// 聊天消息
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChatMessage {
    /// 消息角色（user/assistant/system）
    pub role: MessageRole,

    /// 消息内容
    pub content: String,
}

/// 消息角色
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// 用户消息
    User,

    /// 助手消息
    Assistant,

    /// 系统消息
    System,
}

/// `JiaClaw` 聊天请求
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChatRequest {
    /// 会话历史
    pub messages: Vec<ChatMessage>,

    /// 启用的工具列表（可选）
    #[serde(default)]
    pub enabled_tools: Vec<String>,

    /// 启用的技能列表（可选）
    #[serde(default)]
    pub enabled_skills: Vec<String>,

    /// 是否启用技能自动激活（默认 true）
    #[serde(default = "default_auto_skills")]
    pub auto_skills: bool,

    /// 会话 ID（可选，用于多轮对话）
    #[serde(default)]
    pub session_id: Option<String>,
}

fn default_auto_skills() -> bool {
    true
}

/// `JiaClaw` 聊天响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChatResponse {
    /// 助手回复消息
    pub message: ChatMessage,

    /// 使用的工具调用（可选）
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,

    /// 运行状态
    pub status: RunStatus,

    /// 会话 ID（如果请求中提供）
    #[serde(default)]
    pub session_id: Option<String>,
}

/// 工具调用记录
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolCall {
    /// 工具名称
    pub tool_name: String,

    /// 工具参数
    pub arguments: serde_json::Value,

    /// 工具结果
    pub result: Option<serde_json::Value>,
}

/// 运行状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    /// 运行中
    Running,

    /// 已完成
    Completed,

    /// 失败
    Failed,

    /// 需要人工介入
    RequiresHumanInput,
}

/// Agent 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Agent 名称
    pub name: String,

    /// Agent 描述
    pub description: String,

    /// 系统指令
    pub system_instructions: String,

    /// 最大对话轮次
    pub max_turns: usize,

    /// 工作空间路径
    #[serde(default = "default_workspace_path")]
    pub workspace_path: std::path::PathBuf,

    /// 模型提供商配置
    #[serde(default)]
    pub provider: ProviderConfig,

    /// HTTP 服务配置
    #[serde(default)]
    pub http: HttpConfig,

    /// 工作区长期记忆配置（缺省为 `{workspace}/MEMORY.md`）
    #[serde(default)]
    pub memory: MemoryConfig,
}

fn default_workspace_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".jiaclaw")
        .join("workspace")
}

/// 默认工作区记忆文件名（相对于 `workspace_path`）
pub const DEFAULT_MEMORY_PATH: &str = "MEMORY.md";

/// 注入系统提示时的最大字节数（32 KiB）
pub const MEMORY_PROMPT_MAX_BYTES: usize = 32 * 1024;

fn default_memory_path() -> String {
    DEFAULT_MEMORY_PATH.to_string()
}

/// 工作区长期记忆配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// 记忆文件路径（相对于 `workspace_path`，默认 `MEMORY.md`）
    #[serde(default = "default_memory_path")]
    pub path: String,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            path: default_memory_path(),
        }
    }
}

/// HTTP 服务配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    /// HTTP 服务绑定地址
    #[serde(default = "default_http_bind")]
    pub bind: String,

    /// API 鉴权 Token（可选，环境变量 `JIACLAW_API_TOKEN` 优先）
    #[serde(default)]
    pub api_token: Option<String>,

    /// Webhook 鉴权密钥（可选，环境变量 `JIACLAW_WEBHOOK_SECRET` 优先）
    #[serde(default)]
    pub webhook_secret: Option<String>,

    /// CORS 允许的来源列表（空或 `["*"]` 表示允许所有来源）
    #[serde(default = "default_cors_allow_origins")]
    pub cors_allow_origins: Vec<String>,

    /// 是否持久化 session 到磁盘
    #[serde(default)]
    pub persist: bool,

    /// Session 持久化文件路径（相对于 `workspace_path`）
    #[serde(default = "default_persist_path")]
    pub persist_path: String,

    /// 每分钟全局限流（可选，环境变量 `JIACLAW_RATE_LIMIT_PER_MINUTE` 优先）
    ///
    /// `None` 或非正整数表示不限流。
    #[serde(default)]
    pub rate_limit_per_minute: Option<u32>,
}

fn default_http_bind() -> String {
    "127.0.0.1:8080".to_string()
}

fn default_cors_allow_origins() -> Vec<String> {
    vec!["*".to_string()]
}

fn default_persist_path() -> String {
    ".jiaclaw/sessions.json".to_string()
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            bind: default_http_bind(),
            api_token: None,
            webhook_secret: None,
            cors_allow_origins: default_cors_allow_origins(),
            persist: false,
            persist_path: default_persist_path(),
            rate_limit_per_minute: None,
        }
    }
}

/// 解析正整数限流值；`0` 或无法解析时视为不限流。
#[must_use]
pub fn parse_positive_rate_limit(raw: &str) -> Option<u32> {
    raw.trim().parse::<u32>().ok().filter(|&n| n > 0)
}

/// 根据配置文件值与可选环境变量解析每分钟限流。
///
/// 环境变量优先；仅正整数生效。
#[must_use]
pub fn resolve_rate_limit_per_minute(
    configured: Option<u32>,
    env_value: Option<&str>,
) -> Option<u32> {
    match env_value {
        Some(raw) => parse_positive_rate_limit(raw),
        None => configured.filter(|&n| n > 0),
    }
}

impl HttpConfig {
    /// 解析生效的每分钟请求上限。
    ///
    /// 环境变量 `JIACLAW_RATE_LIMIT_PER_MINUTE` 优先于配置文件。
    /// 仅正整数生效；未设置、`0` 或无法解析表示不限流。
    #[must_use]
    pub fn effective_rate_limit_per_minute(&self) -> Option<u32> {
        resolve_rate_limit_per_minute(
            self.rate_limit_per_minute,
            std::env::var("JIACLAW_RATE_LIMIT_PER_MINUTE")
                .ok()
                .as_deref(),
        )
    }
}

/// 模型提供商配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// 提供商类型 (`brokerrouter` 推荐, `openai_compatible` 已废弃)
    #[serde(default = "default_provider_type")]
    pub provider_type: String,

    /// API 基础 URL（支持 OpenAI、Ollama、LM Studio 等）
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// API 密钥（可选，无密钥时回退到存根）
    #[serde(default)]
    pub api_key: Option<String>,

    /// 模型名称
    #[serde(default = "default_model")]
    pub model: String,

    /// 温度参数
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// 最大 tokens
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn default_provider_type() -> String {
    "brokerrouter".to_string()
}

fn default_base_url() -> String {
    "https://api.brokerrouter.dev".to_string()
}

fn default_model() -> String {
    "gpt-4o-mini".to_string()
}

fn default_temperature() -> f32 {
    0.7
}

fn default_max_tokens() -> u32 {
    4096
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            provider_type: default_provider_type(),
            base_url: default_base_url(),
            api_key: None,
            model: default_model(),
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: "JiaClaw".to_string(),
            description: "Personal durable agent runtime".to_string(),
            system_instructions: "You are JiaClaw, a helpful personal assistant.".to_string(),
            max_turns: 10,
            workspace_path: default_workspace_path(),
            provider: ProviderConfig::default(),
            http: HttpConfig::default(),
            memory: MemoryConfig::default(),
        }
    }
}

impl AgentConfig {
    /// 从 TOML 文件加载配置
    ///
    /// # Errors
    ///
    /// 如果文件读取失败或解析失败，返回错误。
    pub fn from_toml_file(path: impl AsRef<std::path::Path>) -> Result<Self, JiaClawError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| JiaClawError::Configuration(format!("无法读取配置文件: {e}")))?;
        Self::from_toml_str(&content)
    }

    /// 从 TOML 字符串解析配置
    ///
    /// # Errors
    ///
    /// 如果解析失败，返回错误。
    pub fn from_toml_str(content: &str) -> Result<Self, JiaClawError> {
        #[derive(Deserialize)]
        struct ConfigFile {
            agent: AgentConfig,
            #[serde(default)]
            provider: Option<ProviderConfig>,
            #[serde(default)]
            http: Option<HttpConfig>,
            #[serde(default)]
            memory: Option<MemoryConfig>,
        }

        let mut config_file: ConfigFile = toml::from_str(content)
            .map_err(|e| JiaClawError::Configuration(format!("无法解析 TOML 配置: {e}")))?;

        // 如果顶层有 provider / http / memory 配置，覆盖 agent 中的配置
        if let Some(provider) = config_file.provider {
            config_file.agent.provider = provider;
        }
        if let Some(http) = config_file.http {
            config_file.agent.http = http;
        }
        if let Some(memory) = config_file.memory {
            config_file.agent.memory = memory;
        }

        Ok(config_file.agent)
    }

    /// 从 JSON 文件加载配置
    ///
    /// # Errors
    ///
    /// 如果文件读取失败或解析失败，返回错误。
    pub fn from_json_file(path: impl AsRef<std::path::Path>) -> Result<Self, JiaClawError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| JiaClawError::Configuration(format!("无法读取配置文件: {e}")))?;
        Self::from_json_str(&content)
    }

    /// 从 JSON 字符串解析配置
    ///
    /// # Errors
    ///
    /// 如果解析失败，返回错误。
    pub fn from_json_str(content: &str) -> Result<Self, JiaClawError> {
        #[derive(Deserialize)]
        struct ConfigFile {
            agent: AgentConfig,
            #[serde(default)]
            provider: Option<ProviderConfig>,
            #[serde(default)]
            http: Option<HttpConfig>,
            #[serde(default)]
            memory: Option<MemoryConfig>,
        }

        let mut config_file: ConfigFile = serde_json::from_str(content)
            .map_err(|e| JiaClawError::Configuration(format!("无法解析 JSON 配置: {e}")))?;

        // 如果顶层有 provider / http / memory 配置，覆盖 agent 中的配置
        if let Some(provider) = config_file.provider {
            config_file.agent.provider = provider;
        }
        if let Some(http) = config_file.http {
            config_file.agent.http = http;
        }
        if let Some(memory) = config_file.memory {
            config_file.agent.memory = memory;
        }

        Ok(config_file.agent)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_positive_rate_limit, resolve_rate_limit_per_minute, AgentConfig, HttpConfig,
        DEFAULT_MEMORY_PATH,
    };

    #[test]
    fn http_config_rate_limit_defaults_to_none() {
        assert_eq!(HttpConfig::default().rate_limit_per_minute, None);
        assert_eq!(
            HttpConfig::default().effective_rate_limit_per_minute(),
            None
        );
    }

    #[test]
    fn parse_positive_rate_limit_accepts_only_positive_integers() {
        assert_eq!(parse_positive_rate_limit("60"), Some(60));
        assert_eq!(parse_positive_rate_limit(" 1 "), Some(1));
        assert_eq!(parse_positive_rate_limit("0"), None);
        assert_eq!(parse_positive_rate_limit(""), None);
        assert_eq!(parse_positive_rate_limit("abc"), None);
        assert_eq!(parse_positive_rate_limit("-1"), None);
    }

    #[test]
    fn resolve_rate_limit_env_overrides_config() {
        assert_eq!(
            resolve_rate_limit_per_minute(Some(30), Some("120")),
            Some(120)
        );
        assert_eq!(resolve_rate_limit_per_minute(Some(30), Some("0")), None);
        assert_eq!(resolve_rate_limit_per_minute(Some(30), Some("nope")), None);
        assert_eq!(resolve_rate_limit_per_minute(Some(30), None), Some(30));
        assert_eq!(resolve_rate_limit_per_minute(Some(0), None), None);
        assert_eq!(resolve_rate_limit_per_minute(None, None), None);
    }

    #[test]
    fn http_config_parses_rate_limit_from_toml() {
        let toml = r#"
[agent]
name = "JiaClaw"
description = "test"
system_instructions = "be helpful"
max_turns = 10

[http]
bind = "127.0.0.1:9090"
rate_limit_per_minute = 60
"#;
        let config = AgentConfig::from_toml_str(toml).expect("parse toml");
        assert_eq!(config.http.bind, "127.0.0.1:9090");
        assert_eq!(config.http.rate_limit_per_minute, Some(60));
        assert_eq!(config.http.api_token, None);
        assert_eq!(config.http.webhook_secret, None);
    }

    #[test]
    fn http_config_parses_missing_rate_limit_from_json() {
        let json = r#"{
            "agent": {
                "name": "JiaClaw",
                "description": "test",
                "system_instructions": "be helpful",
                "max_turns": 10
            },
            "http": {
                "bind": "0.0.0.0:8080"
            }
        }"#;
        let config = AgentConfig::from_json_str(json).expect("parse json");
        assert_eq!(config.http.bind, "0.0.0.0:8080");
        assert_eq!(config.http.rate_limit_per_minute, None);
        assert_eq!(config.memory.path, DEFAULT_MEMORY_PATH);
    }

    #[test]
    fn memory_config_defaults_without_section() {
        let toml = r#"
[agent]
name = "JiaClaw"
description = "test"
system_instructions = "be helpful"
max_turns = 10
"#;
        let config = AgentConfig::from_toml_str(toml).expect("parse toml");
        assert_eq!(config.memory.path, DEFAULT_MEMORY_PATH);
    }

    #[test]
    fn memory_config_parses_top_level_section() {
        let toml = r#"
[agent]
name = "JiaClaw"
description = "test"
system_instructions = "be helpful"
max_turns = 10

[memory]
path = "notes/MEMORY.md"
"#;
        let config = AgentConfig::from_toml_str(toml).expect("parse toml");
        assert_eq!(config.memory.path, "notes/MEMORY.md");
    }

    #[test]
    fn memory_config_parses_from_json() {
        let json = r#"{
            "agent": {
                "name": "JiaClaw",
                "description": "test",
                "system_instructions": "be helpful",
                "max_turns": 10
            },
            "memory": {
                "path": "custom.md"
            }
        }"#;
        let config = AgentConfig::from_json_str(json).expect("parse json");
        assert_eq!(config.memory.path, "custom.md");
    }
}
