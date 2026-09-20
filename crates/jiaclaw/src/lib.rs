// Copyright 2026 JiaClaw contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `JiaClaw` - 基于 `StateKnot` 的个人持久化智能体运行时
//!
//! `JiaClaw` 是一个"爪式"（Claw-style）个人助手智能体运行时，提供：
//! - 聊天驱动的交互界面
//! - 工具使用能力
//! - 技能系统
//! - 持久化运行（支持重启后恢复）

pub use jiaclaw_core::{
    AgentConfig, ChatMessage, ChatRequest, ChatResponse, JiaClawError, MessageRole, RunStatus,
    ToolCall,
};

// StateKnot imports - commented out until edition 2024 support
// use stateknot_core::{AgentExecutionConfig, AgentInstructions, BudgetLimits};
// use stateknot_runtime::AgentBuilder;

/// `JiaClaw` Agent 包装器
///
/// 当前实现状态：正在等待 `StateKnot` 稳定的公共 API。
/// 本结构体为未来集成预留了接口。
pub struct JiaClawAgent {
    config: AgentConfig,
    // TODO: 当 StateKnot 发布稳定 API 后，添加 TypedAgent 字段
    // typed_agent: TypedAgent<ChatRequest, ChatResponse>,
}

impl JiaClawAgent {
    /// 创建新的 `JiaClaw` Agent 实例
    ///
    /// # Errors
    ///
    /// 当前实现始终返回 `Ok`，但未来可能在以下情况返回错误：
    /// - 配置验证失败
    /// - `StateKnot` 初始化失败
    ///
    /// # 当前限制
    ///
    /// `StateKnot` 当前处于 pre-alpha 阶段，其核心类型尚未发布。
    /// 本方法创建配置，但完整的 `StateKnot` 集成需要等待：
    /// - 稳定的 `AgentBuilder` API
    /// - 发布的 `TypedAgent` 类型
    /// - `DurableAgentAdmission` 边界
    ///
    /// 参见 `docs/stateknot-gaps.md` 了解详情。
    pub fn new(config: AgentConfig) -> Result<Self, JiaClawError> {
        Ok(Self { config })
    }

    /// 获取 Agent 配置
    #[must_use]
    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    /// 处理聊天请求
    ///
    /// # Errors
    ///
    /// 当前实现始终返回 `Ok`，但未来可能在以下情况返回错误：
    /// - 请求验证失败
    /// - `StateKnot` 执行失败
    /// - 模型调用失败
    /// - 持久化失败
    ///
    /// # 当前实现
    ///
    /// 这是一个存根实现。完整的实现需要：
    /// 1. `StateKnot` 的持久化准入（`DurableAgentAdmission`）
    /// 2. 图驱动器（`GraphDriver`）执行
    /// 3. 持久化调用执行器（`DurableInvocationExecutor`）
    /// 4. 模型和工具适配器
    ///
    /// 参见 `StateKnot` 议题：
    /// - 稳定公共 API 发布跟踪
    /// - 简化的 Agent 运行 API
    pub fn chat(
        &self,
        _request: ChatRequest,
    ) -> Result<ChatResponse, JiaClawError> {
        // 存根实现：返回占位响应
        Ok(ChatResponse {
            message: ChatMessage {
                role: MessageRole::Assistant,
                content: format!(
                    "JiaClaw Agent '{}' 已初始化。StateKnot 集成正在等待稳定 API 发布。",
                    self.config.name
                ),
            },
            tool_calls: vec![],
            status: RunStatus::Completed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_creation() {
        let config = AgentConfig::default();
        let agent = JiaClawAgent::new(config);
        assert!(agent.is_ok());
    }

    #[test]
    fn test_chat_stub() {
        let config = AgentConfig::default();
        let agent = JiaClawAgent::new(config).unwrap();
        
        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "你好".to_string(),
            }],
            enabled_tools: vec![],
            enabled_skills: vec![],
        };

        let response = agent.chat(request);
        assert!(response.is_ok());
    }
}
