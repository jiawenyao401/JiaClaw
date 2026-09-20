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
    /// 这是一个存根实现，提供简单的演示响应。完整的实现需要：
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
        request: &ChatRequest,
    ) -> Result<ChatResponse, JiaClawError> {
        // 存根实现：生成基于上下文的响应
        
        // 获取最后一条用户消息
        let last_user_message = request
            .messages
            .iter()
            .rev()
            .find(|msg| matches!(msg.role, MessageRole::User))
            .map_or("", |msg| msg.content.as_str());

        // 简单的响应生成逻辑（演示用）
        let response_content = self.generate_stub_response(last_user_message, request);

        Ok(ChatResponse {
            message: ChatMessage {
                role: MessageRole::Assistant,
                content: response_content,
            },
            tool_calls: vec![],
            status: RunStatus::Completed,
        })
    }

    /// 生成存根响应（演示用）
    fn generate_stub_response(&self, user_message: &str, request: &ChatRequest) -> String {
        let user_lower = user_message.to_lowercase();

        // 简单的关键词匹配响应
        if user_lower.contains("你好") || user_lower.contains("hello") || user_lower.contains("hi") {
            format!(
                "你好！我是 {}。{}\n\n\
                 我目前运行在存根模式下，等待 StateKnot 框架集成。\n\
                 当前配置的最大对话轮次为 {} 轮。\n\n\
                 你可以继续与我对话，我会尽力回应（虽然功能有限）。",
                self.config.name,
                self.config.description,
                self.config.max_turns
            )
        } else if user_lower.contains("功能") || user_lower.contains("能力") || user_lower.contains("what can you do") {
            let tools_str = if request.enabled_tools.is_empty() {
                "无".to_string()
            } else {
                request.enabled_tools.join(", ")
            };
            let skills_str = if request.enabled_skills.is_empty() {
                "无".to_string()
            } else {
                request.enabled_skills.join(", ")
            };
            
            format!(
                "{} 设计用于提供以下能力：\n\n\
                 ✨ 核心功能（计划中）：\n\
                 • 💬 聊天驱动交互 - 自然语言对话\n\
                 • 🔧 工具使用 - 集成外部工具和服务\n\
                 • 🎯 技能系统 - 可扩展的技能模块\n\
                 • 💾 持久化运行 - 支持重启后恢复\n\n\
                 ⚙️ 当前状态：\n\
                 • 存根实现，等待 StateKnot 集成\n\
                 • 已启用工具：{}\n\
                 • 已启用技能：{}\n\n\
                 查看文档了解更多：docs/architecture.md",
                self.config.name,
                tools_str,
                skills_str
            )
        } else if user_lower.contains("帮助") || user_lower.contains("help") {
            "📖 JiaClaw 帮助\n\n\
             命令:\n\
             • `jiaclaw version` - 显示版本信息\n\
             • `jiaclaw chat <消息>` - 发送单次聊天消息\n\
             • `jiaclaw serve` - 启动 HTTP 服务（计划中）\n\n\
             配置:\n\
             • 使用 `--config <文件>` 指定配置文件\n\
             • 支持 TOML 和 JSON 格式\n\
             • 参见 config/jiaclaw.toml.example\n\n\
             文档:\n\
             • README.md - 项目概览\n\
             • docs/architecture.md - 架构设计\n\
             • docs/roadmap.md - 开发路线图".to_string()
        } else if user_lower.contains("状态") || user_lower.contains("status") {
            format!(
                "🔍 {} 状态报告\n\n\
                 Agent 配置:\n\
                 • 名称: {}\n\
                 • 描述: {}\n\
                 • 最大轮次: {}\n\n\
                 会话信息:\n\
                 • 历史消息数: {}\n\
                 • 启用工具数: {}\n\
                 • 启用技能数: {}\n\n\
                 系统状态:\n\
                 • 运行模式: 存根（Stub）\n\
                 • StateKnot 集成: 等待中\n\
                 • 持久化: 未启用",
                self.config.name,
                self.config.name,
                self.config.description,
                self.config.max_turns,
                request.messages.len(),
                request.enabled_tools.len(),
                request.enabled_skills.len()
            )
        } else if user_message.trim().is_empty() {
            "请发送一条消息开始对话。你可以说\"你好\"或询问\"你有什么功能\"。".to_string()
        } else {
            // 默认响应
            format!(
                "我收到了你的消息：\"{user_message}\"\n\n\
                 目前我运行在存根模式下，无法进行实际的自然语言理解或生成。\
                 当 StateKnot 框架集成完成后，我将能够：\n\
                 • 理解复杂的自然语言输入\n\
                 • 使用模型生成智能回复\n\
                 • 调用工具完成实际任务\n\
                 • 保持持久化的对话上下文\n\n\
                 试试说\"帮助\"了解更多命令。"
            )
        }
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

        let response = agent.chat(&request);
        assert!(response.is_ok());
    }
}
