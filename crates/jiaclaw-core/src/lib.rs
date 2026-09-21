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

    /// 工作区人格 / 用户画像配置（缺省为 `{workspace}/SOUL.md` 与 `USER.md`）
    #[serde(default)]
    pub identity: IdentityConfig,

    /// 工作区心跳配置（缺省关闭；约定文件 `{workspace}/HEARTBEAT.md`）
    #[serde(default)]
    pub heartbeat: HeartbeatConfig,

    /// 单次工具调用超时（秒，可选，环境变量 `JIACLAW_TOOL_TIMEOUT_SECS` 优先）
    ///
    /// `None` 或非正整数表示不限制，保持现有行为。
    #[serde(default)]
    pub tool_timeout_secs: Option<u64>,
}

fn default_workspace_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".jiaclaw")
        .join("workspace")
}

/// 默认工作区记忆文件名（相对于 `workspace_path`）
pub const DEFAULT_MEMORY_PATH: &str = "MEMORY.md";

/// 默认人格文件名（相对于 `workspace_path`）
pub const DEFAULT_SOUL_PATH: &str = "SOUL.md";

/// 默认用户画像文件名（相对于 `workspace_path`）
pub const DEFAULT_USER_PATH: &str = "USER.md";

/// 默认心跳文件名（相对于 `workspace_path`）
pub const DEFAULT_HEARTBEAT_PATH: &str = "HEARTBEAT.md";

/// 默认心跳会话 ID（固定会话，便于追踪）
pub const DEFAULT_HEARTBEAT_SESSION_ID: &str = "heartbeat";

/// 默认心跳间隔（秒）
pub const DEFAULT_HEARTBEAT_INTERVAL_SECS: u64 = 3600;

/// 注入系统提示时的最大字节数（32 KiB）；MEMORY / SOUL / USER 各自独立截断
pub const MEMORY_PROMPT_MAX_BYTES: usize = 32 * 1024;

fn default_memory_path() -> String {
    DEFAULT_MEMORY_PATH.to_string()
}

fn default_soul_path() -> String {
    DEFAULT_SOUL_PATH.to_string()
}

fn default_user_path() -> String {
    DEFAULT_USER_PATH.to_string()
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

/// 工作区人格与用户画像配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityConfig {
    /// 人格文件路径（相对于 `workspace_path`，默认 `SOUL.md`）
    #[serde(default = "default_soul_path")]
    pub soul_path: String,

    /// 用户画像文件路径（相对于 `workspace_path`，默认 `USER.md`）
    #[serde(default = "default_user_path")]
    pub user_path: String,
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            soul_path: default_soul_path(),
            user_path: default_user_path(),
        }
    }
}

fn default_heartbeat_path() -> String {
    DEFAULT_HEARTBEAT_PATH.to_string()
}

fn default_heartbeat_session_id() -> String {
    DEFAULT_HEARTBEAT_SESSION_ID.to_string()
}

fn default_heartbeat_interval_secs() -> u64 {
    DEFAULT_HEARTBEAT_INTERVAL_SECS
}

/// 工作区心跳配置（仅 `jiaclaw serve` 进程内生效；默认关闭）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatConfig {
    /// 是否启用定时心跳（默认 `false`）
    #[serde(default)]
    pub enabled: bool,

    /// 心跳间隔（秒，默认 3600）；环境变量 `JIACLAW_HEARTBEAT_INTERVAL_SECS` 可覆盖（正整数）
    #[serde(default = "default_heartbeat_interval_secs")]
    pub interval_secs: u64,

    /// 心跳文件路径（相对于 `workspace_path`，默认 `HEARTBEAT.md`）
    #[serde(default = "default_heartbeat_path")]
    pub path: String,

    /// 固定会话 ID（默认 `heartbeat`），便于追踪
    #[serde(default = "default_heartbeat_session_id")]
    pub session_id: String,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: default_heartbeat_interval_secs(),
            path: default_heartbeat_path(),
            session_id: default_heartbeat_session_id(),
        }
    }
}

impl HeartbeatConfig {
    /// 解析生效的心跳间隔（秒）。
    ///
    /// 环境变量 `JIACLAW_HEARTBEAT_INTERVAL_SECS` 优先于配置文件；仅正整数生效。
    /// 未设置、`0` 或无法解析时回退到配置值；配置值非正则使用默认 3600。
    #[must_use]
    pub fn effective_interval_secs(&self) -> u64 {
        resolve_heartbeat_interval_secs(
            self.interval_secs,
            std::env::var("JIACLAW_HEARTBEAT_INTERVAL_SECS")
                .ok()
                .as_deref(),
        )
    }

    /// 生效的固定会话 ID；空白时回退到 [`DEFAULT_HEARTBEAT_SESSION_ID`]。
    #[must_use]
    pub fn effective_session_id(&self) -> &str {
        let trimmed = self.session_id.trim();
        if trimmed.is_empty() {
            DEFAULT_HEARTBEAT_SESSION_ID
        } else {
            trimmed
        }
    }
}

/// 解析正整数心跳间隔（秒）；`0` 或无法解析时返回 `None`。
#[must_use]
pub fn parse_positive_heartbeat_interval(raw: &str) -> Option<u64> {
    raw.trim().parse::<u64>().ok().filter(|&n| n > 0)
}

/// 根据配置文件值与可选环境变量解析心跳间隔（秒）。
///
/// 环境变量优先（仅正整数）；否则使用正整数配置值；再否则默认 3600。
#[must_use]
pub fn resolve_heartbeat_interval_secs(configured: u64, env_value: Option<&str>) -> u64 {
    match env_value {
        Some(raw) => parse_positive_heartbeat_interval(raw)
            .unwrap_or_else(|| fallback_heartbeat_interval(configured)),
        None => fallback_heartbeat_interval(configured),
    }
}

fn fallback_heartbeat_interval(configured: u64) -> u64 {
    if configured > 0 {
        configured
    } else {
        DEFAULT_HEARTBEAT_INTERVAL_SECS
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

    /// Telegram Bot webhook secret token（可选，环境变量 `JIACLAW_TELEGRAM_SECRET` 优先）
    ///
    /// 对应 Telegram `setWebhook` 的 `secret_token`，校验头 `X-Telegram-Bot-Api-Secret-Token`。
    #[serde(default)]
    pub telegram_secret: Option<String>,

    /// Telegram Bot API token（可选，环境变量 `JIACLAW_TELEGRAM_BOT_TOKEN` 优先）
    ///
    /// 配置后，`POST /hooks/telegram` 在得到 assistant 回复时会调用 Bot `sendMessage` 推回聊天。
    /// 未配置则仅同步 JSON 回传 `reply`（与仅入站切片行为一致）。
    #[serde(default)]
    pub telegram_bot_token: Option<String>,

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

    /// 会话闲置 TTL（秒，可选，环境变量 `JIACLAW_SESSION_TTL_SECS` 优先）
    ///
    /// `None` 或非正整数表示不启用过期清理。
    #[serde(default)]
    pub session_ttl_secs: Option<u64>,
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
            telegram_secret: None,
            telegram_bot_token: None,
            cors_allow_origins: default_cors_allow_origins(),
            persist: false,
            persist_path: default_persist_path(),
            rate_limit_per_minute: None,
            session_ttl_secs: None,
        }
    }
}

/// 解析可选密钥：环境变量优先；空白视为未设置并回退到配置文件。
#[must_use]
pub fn resolve_optional_secret(
    configured: Option<String>,
    env_value: Option<&str>,
) -> Option<String> {
    env_value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            configured
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
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

/// 解析正整数会话 TTL（秒）；`0` 或无法解析时视为不启用。
#[must_use]
pub fn parse_positive_session_ttl(raw: &str) -> Option<u64> {
    raw.trim().parse::<u64>().ok().filter(|&n| n > 0)
}

/// 根据配置文件值与可选环境变量解析会话闲置 TTL（秒）。
///
/// 环境变量优先；仅正整数生效。
#[must_use]
pub fn resolve_session_ttl_secs(configured: Option<u64>, env_value: Option<&str>) -> Option<u64> {
    match env_value {
        Some(raw) => parse_positive_session_ttl(raw),
        None => configured.filter(|&n| n > 0),
    }
}

/// 解析正整数工具超时（秒）；`0` 或无法解析时视为不启用。
#[must_use]
pub fn parse_positive_tool_timeout(raw: &str) -> Option<u64> {
    raw.trim().parse::<u64>().ok().filter(|&n| n > 0)
}

/// 根据配置文件值与可选环境变量解析单次工具调用超时（秒）。
///
/// 环境变量优先；仅正整数生效。
#[must_use]
pub fn resolve_tool_timeout_secs(configured: Option<u64>, env_value: Option<&str>) -> Option<u64> {
    match env_value {
        Some(raw) => parse_positive_tool_timeout(raw),
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

    /// 解析生效的会话闲置 TTL（秒）。
    ///
    /// 环境变量 `JIACLAW_SESSION_TTL_SECS` 优先于配置文件。
    /// 仅正整数生效；未设置、`0` 或无法解析表示不启用。
    #[must_use]
    pub fn effective_session_ttl_secs(&self) -> Option<u64> {
        resolve_session_ttl_secs(
            self.session_ttl_secs,
            std::env::var("JIACLAW_SESSION_TTL_SECS").ok().as_deref(),
        )
    }

    /// 解析生效的 Telegram Bot API token。
    ///
    /// 环境变量 `JIACLAW_TELEGRAM_BOT_TOKEN` 优先于配置文件；空白视为未配置。
    #[must_use]
    pub fn effective_telegram_bot_token(&self) -> Option<String> {
        resolve_optional_secret(
            self.telegram_bot_token.clone(),
            std::env::var("JIACLAW_TELEGRAM_BOT_TOKEN").ok().as_deref(),
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
            identity: IdentityConfig::default(),
            heartbeat: HeartbeatConfig::default(),
            tool_timeout_secs: None,
        }
    }
}

impl AgentConfig {
    /// 解析生效的单次工具调用超时（秒）。
    ///
    /// 环境变量 `JIACLAW_TOOL_TIMEOUT_SECS` 优先于配置文件。
    /// 仅正整数生效；未设置、`0` 或无法解析表示不限制。
    #[must_use]
    pub fn effective_tool_timeout_secs(&self) -> Option<u64> {
        resolve_tool_timeout_secs(
            self.tool_timeout_secs,
            std::env::var("JIACLAW_TOOL_TIMEOUT_SECS").ok().as_deref(),
        )
    }

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
            #[serde(default)]
            identity: Option<IdentityConfig>,
            #[serde(default)]
            heartbeat: Option<HeartbeatConfig>,
        }

        let mut config_file: ConfigFile = toml::from_str(content)
            .map_err(|e| JiaClawError::Configuration(format!("无法解析 TOML 配置: {e}")))?;

        // 如果顶层有 provider / http / memory / identity / heartbeat 配置，覆盖 agent 中的配置
        if let Some(provider) = config_file.provider {
            config_file.agent.provider = provider;
        }
        if let Some(http) = config_file.http {
            config_file.agent.http = http;
        }
        if let Some(memory) = config_file.memory {
            config_file.agent.memory = memory;
        }
        if let Some(identity) = config_file.identity {
            config_file.agent.identity = identity;
        }
        if let Some(heartbeat) = config_file.heartbeat {
            config_file.agent.heartbeat = heartbeat;
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
            #[serde(default)]
            identity: Option<IdentityConfig>,
            #[serde(default)]
            heartbeat: Option<HeartbeatConfig>,
        }

        let mut config_file: ConfigFile = serde_json::from_str(content)
            .map_err(|e| JiaClawError::Configuration(format!("无法解析 JSON 配置: {e}")))?;

        // 如果顶层有 provider / http / memory / identity / heartbeat 配置，覆盖 agent 中的配置
        if let Some(provider) = config_file.provider {
            config_file.agent.provider = provider;
        }
        if let Some(http) = config_file.http {
            config_file.agent.http = http;
        }
        if let Some(memory) = config_file.memory {
            config_file.agent.memory = memory;
        }
        if let Some(identity) = config_file.identity {
            config_file.agent.identity = identity;
        }
        if let Some(heartbeat) = config_file.heartbeat {
            config_file.agent.heartbeat = heartbeat;
        }

        Ok(config_file.agent)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_positive_heartbeat_interval, parse_positive_rate_limit, parse_positive_session_ttl,
        parse_positive_tool_timeout, resolve_heartbeat_interval_secs, resolve_optional_secret,
        resolve_rate_limit_per_minute, resolve_session_ttl_secs, resolve_tool_timeout_secs,
        AgentConfig, HeartbeatConfig, HttpConfig, DEFAULT_HEARTBEAT_INTERVAL_SECS,
        DEFAULT_HEARTBEAT_PATH, DEFAULT_HEARTBEAT_SESSION_ID, DEFAULT_MEMORY_PATH,
        DEFAULT_SOUL_PATH, DEFAULT_USER_PATH,
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
    fn http_config_session_ttl_defaults_to_none() {
        assert_eq!(HttpConfig::default().session_ttl_secs, None);
        assert_eq!(HttpConfig::default().effective_session_ttl_secs(), None);
    }

    #[test]
    fn agent_config_tool_timeout_defaults_to_none() {
        assert_eq!(AgentConfig::default().tool_timeout_secs, None);
        assert_eq!(AgentConfig::default().effective_tool_timeout_secs(), None);
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
    fn parse_positive_session_ttl_accepts_only_positive_integers() {
        assert_eq!(parse_positive_session_ttl("3600"), Some(3600));
        assert_eq!(parse_positive_session_ttl(" 1 "), Some(1));
        assert_eq!(parse_positive_session_ttl("0"), None);
        assert_eq!(parse_positive_session_ttl(""), None);
        assert_eq!(parse_positive_session_ttl("abc"), None);
        assert_eq!(parse_positive_session_ttl("-1"), None);
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
    fn resolve_session_ttl_env_overrides_config() {
        assert_eq!(resolve_session_ttl_secs(Some(30), Some("120")), Some(120));
        assert_eq!(resolve_session_ttl_secs(Some(30), Some("0")), None);
        assert_eq!(resolve_session_ttl_secs(Some(30), Some("nope")), None);
        assert_eq!(resolve_session_ttl_secs(Some(30), None), Some(30));
        assert_eq!(resolve_session_ttl_secs(Some(0), None), None);
        assert_eq!(resolve_session_ttl_secs(None, None), None);
    }

    #[test]
    fn parse_positive_tool_timeout_accepts_only_positive_integers() {
        assert_eq!(parse_positive_tool_timeout("30"), Some(30));
        assert_eq!(parse_positive_tool_timeout(" 1 "), Some(1));
        assert_eq!(parse_positive_tool_timeout("0"), None);
        assert_eq!(parse_positive_tool_timeout(""), None);
        assert_eq!(parse_positive_tool_timeout("abc"), None);
        assert_eq!(parse_positive_tool_timeout("-1"), None);
    }

    #[test]
    fn resolve_tool_timeout_env_overrides_config() {
        assert_eq!(resolve_tool_timeout_secs(Some(30), Some("120")), Some(120));
        assert_eq!(resolve_tool_timeout_secs(Some(30), Some("0")), None);
        assert_eq!(resolve_tool_timeout_secs(Some(30), Some("nope")), None);
        assert_eq!(resolve_tool_timeout_secs(Some(30), None), Some(30));
        assert_eq!(resolve_tool_timeout_secs(Some(0), None), None);
        assert_eq!(resolve_tool_timeout_secs(None, None), None);
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
session_ttl_secs = 3600
"#;
        let config = AgentConfig::from_toml_str(toml).expect("parse toml");
        assert_eq!(config.http.bind, "127.0.0.1:9090");
        assert_eq!(config.http.rate_limit_per_minute, Some(60));
        assert_eq!(config.http.session_ttl_secs, Some(3600));
        assert_eq!(config.http.api_token, None);
        assert_eq!(config.http.webhook_secret, None);
        assert_eq!(config.http.telegram_secret, None);
        assert_eq!(config.http.telegram_bot_token, None);
        assert_eq!(config.tool_timeout_secs, None);
    }

    #[test]
    fn http_config_telegram_secret_defaults_to_none() {
        assert_eq!(HttpConfig::default().telegram_secret, None);
        assert_eq!(HttpConfig::default().telegram_bot_token, None);
    }

    #[test]
    fn http_config_parses_telegram_secret_from_toml() {
        let toml = r#"
[agent]
name = "JiaClaw"
description = "test"
system_instructions = "be helpful"
max_turns = 10

[http]
bind = "127.0.0.1:8080"
telegram_secret = "tg-secret-token"
"#;
        let config = AgentConfig::from_toml_str(toml).expect("parse toml");
        assert_eq!(
            config.http.telegram_secret.as_deref(),
            Some("tg-secret-token")
        );
        assert_eq!(config.http.webhook_secret, None);
        assert_eq!(config.http.telegram_bot_token, None);
    }

    #[test]
    fn http_config_parses_telegram_bot_token_from_toml() {
        let toml = r#"
[agent]
name = "JiaClaw"
description = "test"
system_instructions = "be helpful"
max_turns = 10

[http]
bind = "127.0.0.1:8080"
telegram_secret = "tg-secret-token"
telegram_bot_token = "123456:ABC-bot-token"
"#;
        let config = AgentConfig::from_toml_str(toml).expect("parse toml");
        assert_eq!(
            config.http.telegram_secret.as_deref(),
            Some("tg-secret-token")
        );
        assert_eq!(
            config.http.telegram_bot_token.as_deref(),
            Some("123456:ABC-bot-token")
        );
    }

    #[test]
    fn resolve_optional_secret_env_overrides_config() {
        assert_eq!(
            resolve_optional_secret(Some("from-file".into()), Some("from-env")),
            Some("from-env".into())
        );
        assert_eq!(
            resolve_optional_secret(Some("from-file".into()), Some("  ")),
            Some("from-file".into())
        );
        assert_eq!(
            resolve_optional_secret(Some("  file-token  ".into()), None),
            Some("file-token".into())
        );
        assert_eq!(resolve_optional_secret(Some(String::new()), None), None);
        assert_eq!(resolve_optional_secret(None, None), None);
        assert_eq!(
            resolve_optional_secret(None, Some(" env-token ")),
            Some("env-token".into())
        );
    }

    #[test]
    fn agent_config_parses_tool_timeout_from_toml() {
        let toml = r#"
[agent]
name = "JiaClaw"
description = "test"
system_instructions = "be helpful"
max_turns = 10
tool_timeout_secs = 30
"#;
        let config = AgentConfig::from_toml_str(toml).expect("parse toml");
        assert_eq!(config.tool_timeout_secs, Some(30));
    }

    #[test]
    fn agent_config_parses_tool_timeout_from_json() {
        let json = r#"{
            "agent": {
                "name": "JiaClaw",
                "description": "test",
                "system_instructions": "be helpful",
                "max_turns": 10,
                "tool_timeout_secs": 15
            }
        }"#;
        let config = AgentConfig::from_json_str(json).expect("parse json");
        assert_eq!(config.tool_timeout_secs, Some(15));
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
        assert_eq!(config.http.session_ttl_secs, None);
        assert_eq!(config.memory.path, DEFAULT_MEMORY_PATH);
        assert_eq!(config.identity.soul_path, DEFAULT_SOUL_PATH);
        assert_eq!(config.identity.user_path, DEFAULT_USER_PATH);
        assert_eq!(config.heartbeat.path, DEFAULT_HEARTBEAT_PATH);
        assert!(!config.heartbeat.enabled);
        assert_eq!(
            config.heartbeat.interval_secs,
            DEFAULT_HEARTBEAT_INTERVAL_SECS
        );
        assert_eq!(config.heartbeat.session_id, DEFAULT_HEARTBEAT_SESSION_ID);
        assert_eq!(config.tool_timeout_secs, None);
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
        assert_eq!(config.identity.soul_path, DEFAULT_SOUL_PATH);
        assert_eq!(config.identity.user_path, DEFAULT_USER_PATH);
        assert!(!config.heartbeat.enabled);
        assert_eq!(config.heartbeat.path, DEFAULT_HEARTBEAT_PATH);
        assert_eq!(
            config.heartbeat.interval_secs,
            DEFAULT_HEARTBEAT_INTERVAL_SECS
        );
        assert_eq!(config.heartbeat.session_id, DEFAULT_HEARTBEAT_SESSION_ID);
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

    #[test]
    fn identity_config_defaults_without_section() {
        let toml = r#"
[agent]
name = "JiaClaw"
description = "test"
system_instructions = "be helpful"
max_turns = 10
"#;
        let config = AgentConfig::from_toml_str(toml).expect("parse toml");
        assert_eq!(config.identity.soul_path, DEFAULT_SOUL_PATH);
        assert_eq!(config.identity.user_path, DEFAULT_USER_PATH);
    }

    #[test]
    fn identity_config_parses_top_level_section() {
        let toml = r#"
[agent]
name = "JiaClaw"
description = "test"
system_instructions = "be helpful"
max_turns = 10

[identity]
soul_path = "persona/SOUL.md"
user_path = "persona/USER.md"
"#;
        let config = AgentConfig::from_toml_str(toml).expect("parse toml");
        assert_eq!(config.identity.soul_path, "persona/SOUL.md");
        assert_eq!(config.identity.user_path, "persona/USER.md");
    }

    #[test]
    fn identity_config_parses_from_json() {
        let json = r#"{
            "agent": {
                "name": "JiaClaw",
                "description": "test",
                "system_instructions": "be helpful",
                "max_turns": 10
            },
            "identity": {
                "soul_path": "soul.md",
                "user_path": "user.md"
            }
        }"#;
        let config = AgentConfig::from_json_str(json).expect("parse json");
        assert_eq!(config.identity.soul_path, "soul.md");
        assert_eq!(config.identity.user_path, "user.md");
        assert_eq!(config.memory.path, DEFAULT_MEMORY_PATH);
        assert!(!config.heartbeat.enabled);
        assert_eq!(config.heartbeat.path, DEFAULT_HEARTBEAT_PATH);
    }

    #[test]
    fn heartbeat_config_defaults_are_disabled() {
        let cfg = HeartbeatConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.interval_secs, DEFAULT_HEARTBEAT_INTERVAL_SECS);
        assert_eq!(cfg.path, DEFAULT_HEARTBEAT_PATH);
        assert_eq!(cfg.session_id, DEFAULT_HEARTBEAT_SESSION_ID);
        assert_eq!(cfg.effective_session_id(), DEFAULT_HEARTBEAT_SESSION_ID);
    }

    #[test]
    fn parse_positive_heartbeat_interval_accepts_only_positive_integers() {
        assert_eq!(parse_positive_heartbeat_interval("3600"), Some(3600));
        assert_eq!(parse_positive_heartbeat_interval(" 1 "), Some(1));
        assert_eq!(parse_positive_heartbeat_interval("0"), None);
        assert_eq!(parse_positive_heartbeat_interval(""), None);
        assert_eq!(parse_positive_heartbeat_interval("abc"), None);
        assert_eq!(parse_positive_heartbeat_interval("-1"), None);
    }

    #[test]
    fn resolve_heartbeat_interval_env_overrides_config() {
        assert_eq!(resolve_heartbeat_interval_secs(30, Some("120")), 120);
        assert_eq!(resolve_heartbeat_interval_secs(30, Some("0")), 30);
        assert_eq!(resolve_heartbeat_interval_secs(30, Some("nope")), 30);
        assert_eq!(resolve_heartbeat_interval_secs(30, None), 30);
        assert_eq!(
            resolve_heartbeat_interval_secs(0, None),
            DEFAULT_HEARTBEAT_INTERVAL_SECS
        );
        assert_eq!(
            resolve_heartbeat_interval_secs(0, Some("abc")),
            DEFAULT_HEARTBEAT_INTERVAL_SECS
        );
    }

    #[test]
    fn heartbeat_config_parses_top_level_section() {
        let toml = r#"
[agent]
name = "JiaClaw"
description = "test"
system_instructions = "be helpful"
max_turns = 10

[heartbeat]
enabled = true
interval_secs = 15
path = "ops/HEARTBEAT.md"
session_id = "nightly"
"#;
        let config = AgentConfig::from_toml_str(toml).expect("parse toml");
        assert!(config.heartbeat.enabled);
        assert_eq!(config.heartbeat.interval_secs, 15);
        assert_eq!(config.heartbeat.path, "ops/HEARTBEAT.md");
        assert_eq!(config.heartbeat.session_id, "nightly");
        assert_eq!(config.heartbeat.effective_session_id(), "nightly");
        assert_eq!(config.memory.path, DEFAULT_MEMORY_PATH);
    }

    #[test]
    fn heartbeat_config_parses_from_json() {
        let json = r#"{
            "agent": {
                "name": "JiaClaw",
                "description": "test",
                "system_instructions": "be helpful",
                "max_turns": 10
            },
            "heartbeat": {
                "enabled": true,
                "interval_secs": 90,
                "path": "pulse.md",
                "session_id": "pulse"
            }
        }"#;
        let config = AgentConfig::from_json_str(json).expect("parse json");
        assert!(config.heartbeat.enabled);
        assert_eq!(config.heartbeat.interval_secs, 90);
        assert_eq!(config.heartbeat.path, "pulse.md");
        assert_eq!(config.heartbeat.session_id, "pulse");
        assert_eq!(config.identity.soul_path, DEFAULT_SOUL_PATH);
    }

    #[test]
    fn heartbeat_blank_session_id_falls_back() {
        let cfg = HeartbeatConfig {
            session_id: "  ".to_string(),
            ..HeartbeatConfig::default()
        };
        assert_eq!(cfg.effective_session_id(), DEFAULT_HEARTBEAT_SESSION_ID);
    }
}
