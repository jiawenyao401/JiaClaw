// Copyright 2026 JiaClaw contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `JiaClaw` 核心领域类型和契约
//!
//! 本模块定义了 `JiaClaw` 个人智能体运行时的核心类型。

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: "JiaClaw".to_string(),
            description: "Personal durable agent runtime".to_string(),
            system_instructions: "You are JiaClaw, a helpful personal assistant.".to_string(),
            max_turns: 10,
        }
    }
}
