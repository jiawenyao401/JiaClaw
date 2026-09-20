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
    AgentConfig, ChatMessage, ChatRequest, ChatResponse, JiaClawError, MessageRole, ProviderConfig,
    RunStatus, ToolCall,
};

mod provider;
mod workspace;

use provider::{BrokerrouterProvider, OpenAICompatibleProvider};
pub use workspace::Workspace;

// StateKnot imports - commented out until edition 2024 support
// use stateknot_core::{AgentExecutionConfig, AgentInstructions, BudgetLimits};
// use stateknot_runtime::AgentBuilder;

/// `JiaClaw` Agent 包装器
///
/// 当前实现状态：正在等待 `StateKnot` 稳定的公共 API。
/// 本结构体为未来集成预留了接口。
pub struct JiaClawAgent {
    config: AgentConfig,
    workspace: Workspace,
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
        // 加载工作空间文件
        let workspace = Workspace::load(&config.workspace_path)?;

        Ok(Self { config, workspace })
    }

    /// 获取 Agent 配置
    #[must_use]
    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    /// 获取工作空间
    #[must_use]
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// 处理聊天请求
    ///
    /// # Errors
    ///
    /// 可能返回以下错误：
    /// - 请求验证失败
    /// - 模型调用失败
    /// - 网络错误
    ///
    /// # 实现说明
    ///
    /// 根据配置选择提供商：
    /// 1. `brokerrouter` - 推荐的生产路径（通过 Brokerrouter Gateway）
    /// 2. `openai_compatible` - 已废弃的直连模式（仅作开发逃生舱）
    /// 3. 如果未配置 API key，回退到存根实现
    pub async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, JiaClawError> {
        // 构建完整的系统提示（包含工作空间内容）
        let system_prompt = self.build_system_prompt(request);

        // 从配置或环境变量获取 API key
        let env_key = std::env::var("JIACLAW_API_KEY").ok();
        let api_key = self
            .config
            .provider
            .api_key
            .as_deref()
            .or(env_key.as_deref());

        // 选择提供商
        match (api_key, self.config.provider.provider_type.as_str()) {
            (Some(key), "brokerrouter") => {
                // 推荐：Brokerrouter 提供商
                let provider = BrokerrouterProvider::new(&self.config.provider.base_url, key);
                provider
                    .chat(
                        &self.config.provider.model,
                        &system_prompt,
                        &request.messages,
                        self.config.provider.temperature,
                        self.config.provider.max_tokens,
                    )
                    .await
            }
            (Some(key), "openai_compatible") => {
                // 已废弃：OpenAI-compatible 提供商
                let provider = OpenAICompatibleProvider::new(&self.config.provider.base_url, key);
                provider
                    .chat(
                        &self.config.provider.model,
                        &system_prompt,
                        &request.messages,
                        self.config.provider.temperature,
                        self.config.provider.max_tokens,
                    )
                    .await
            }
            (Some(_), unknown_type) => {
                tracing::warn!(
                    "未知的提供商类型 '{}', 回退到存根模式",
                    unknown_type
                );
                Ok(self.stub_chat(request, &system_prompt))
            }
            (None, _) => {
                // 回退到存根实现
                tracing::warn!(
                    "未配置 API key（通过配置文件或 JIACLAW_API_KEY 环境变量），使用存根模式"
                );
                Ok(self.stub_chat(request, &system_prompt))
            }
        }
    }

    /// 构建系统提示（包含工作空间内容）
    fn build_system_prompt(&self, request: &ChatRequest) -> String {
        let mut prompt = self.config.system_instructions.clone();

        // 添加工作空间内容
        if let Some(ref soul) = self.workspace.soul {
            prompt.push_str("\n\n## Agent Soul\n");
            prompt.push_str(soul);
        }

        if let Some(ref user) = self.workspace.user {
            prompt.push_str("\n\n## User Profile\n");
            prompt.push_str(user);
        }

        if let Some(ref memory) = self.workspace.memory {
            prompt.push_str("\n\n## Long-term Memory\n");
            prompt.push_str(memory);
        }

        // 添加技能摘要
        if !request.enabled_skills.is_empty() {
            prompt.push_str("\n\n## Enabled Skills\n");
            for skill in &request.enabled_skills {
                prompt.push_str(&format!("- {skill}\n"));
            }
        }

        prompt
    }

    /// 存根实现（无 API key 时使用）
    fn stub_chat(&self, request: &ChatRequest, system_prompt: &str) -> ChatResponse {
        // 获取最后一条用户消息
        let last_user_message = request
            .messages
            .iter()
            .rev()
            .find(|msg| matches!(msg.role, MessageRole::User))
            .map_or("", |msg| msg.content.as_str());

        // 简单的响应生成逻辑（演示用）
        let response_content =
            self.generate_stub_response(last_user_message, request, system_prompt);

        ChatResponse {
            message: ChatMessage {
                role: MessageRole::Assistant,
                content: response_content,
            },
            tool_calls: vec![],
            status: RunStatus::Completed,
        }
    }

    /// 生成存根响应（演示用）
    fn generate_stub_response(
        &self,
        user_message: &str,
        request: &ChatRequest,
        _system_prompt: &str,
    ) -> String {
        let user_lower = user_message.to_lowercase();

        // 简单的关键词匹配响应
        if user_lower.contains("你好") || user_lower.contains("hello") || user_lower.contains("hi")
        {
            format!(
                "你好！我是 {}。{}\n\n\
                 我目前运行在存根模式下，等待 StateKnot 框架集成。\n\
                 当前配置的最大对话轮次为 {} 轮。\n\n\
                 你可以继续与我对话，我会尽力回应（虽然功能有限）。",
                self.config.name, self.config.description, self.config.max_turns
            )
        } else if user_lower.contains("功能")
            || user_lower.contains("能力")
            || user_lower.contains("what can you do")
        {
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
                self.config.name, tools_str, skills_str
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
             • docs/roadmap.md - 开发路线图"
                .to_string()
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

    #[tokio::test]
    async fn test_chat_stub() {
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

        let response = agent.chat(&request).await;
        assert!(response.is_ok());
    }
}
