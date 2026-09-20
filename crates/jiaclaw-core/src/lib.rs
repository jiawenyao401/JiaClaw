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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
}

fn default_workspace_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".jiaclaw")
        .join("workspace")
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
        }

        let config: ConfigFile = toml::from_str(content)
            .map_err(|e| JiaClawError::Configuration(format!("无法解析 TOML 配置: {e}")))?;
        Ok(config.agent)
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
        }

        let config: ConfigFile = serde_json::from_str(content)
            .map_err(|e| JiaClawError::Configuration(format!("无法解析 JSON 配置: {e}")))?;
        Ok(config.agent)
    }
}
