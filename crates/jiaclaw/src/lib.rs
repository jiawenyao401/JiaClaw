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
    AgentConfig, ChatMessage, ChatRequest, ChatResponse, HttpConfig, JiaClawError, MessageRole,
    ProviderConfig, RunStatus, ToolCall,
};

mod provider;
mod skills;
mod tools;
mod workspace;

use provider::{BrokerrouterProvider, OpenAICompatibleProvider};
pub use skills::{Skill, SkillDiscovery};
pub use tools::{
    DateTimeTool, FileCopyTool, FileDeleteTool, FileListTool, FileReadTool, FileWriteTool,
    HttpGetTool, JsonQueryTool, MemoryReadTool, ShellExecTool, Tool, ToolRegistry,
    WorkspaceListTool,
};
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
    skills: Vec<Skill>,
    tools: ToolRegistry,
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

        // 发现技能
        let skill_discovery = SkillDiscovery::new(&config.workspace_path);
        let skills = skill_discovery.discover().unwrap_or_else(|e| {
            tracing::warn!("技能发现失败: {e}");
            Vec::new()
        });

        if !skills.is_empty() {
            tracing::info!("发现 {} 个技能", skills.len());
        }

        // 初始化工具注册表
        let mut tools = ToolRegistry::new();

        // 工作空间和记忆工具
        tools.register(Box::new(WorkspaceListTool::new(&config.workspace_path)));
        tools.register(Box::new(MemoryReadTool::new(&config.workspace_path)));

        // 文件操作工具
        tools.register(Box::new(FileReadTool::new(&config.workspace_path)));
        tools.register(Box::new(FileWriteTool::new(&config.workspace_path)));
        tools.register(Box::new(FileListTool::new(&config.workspace_path)));
        tools.register(Box::new(FileDeleteTool::new(&config.workspace_path)));
        tools.register(Box::new(FileCopyTool::new(&config.workspace_path)));

        // 网络和数据工具
        tools.register(Box::new(HttpGetTool::new()));
        tools.register(Box::new(JsonQueryTool::new()));

        // 系统工具
        tools.register(Box::new(DateTimeTool::new()));
        tools.register(Box::new(ShellExecTool::new(&config.workspace_path)));

        tracing::info!("注册了 {} 个本地工具", tools.list().len());

        Ok(Self {
            config,
            workspace,
            skills,
            tools,
        })
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

    /// 获取技能列表
    #[must_use]
    pub fn skills(&self) -> &[Skill] {
        &self.skills
    }

    /// 获取工具注册表
    #[must_use]
    pub fn tools(&self) -> &ToolRegistry {
        &self.tools
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
    ///
    /// 本方法会自动处理工具调用循环（最多 5 次迭代）。
    pub async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, JiaClawError> {
        // 检查是否有技能应该被自动触发（仅在 auto_skills 为 true 时）
        let mut enabled_skills = request.enabled_skills.clone();
        
        if request.auto_skills {
            if let Some(last_user_msg) = request
                .messages
                .iter()
                .rev()
                .find(|m| matches!(m.role, MessageRole::User))
            {
                let discovery = SkillDiscovery::new(&self.config.workspace_path);
                let auto_triggered = discovery.auto_trigger_skills(&last_user_msg.content, &self.skills);
                
                for skill_name in auto_triggered {
                    if !enabled_skills.contains(&skill_name) {
                        tracing::info!("自动激活技能: {}", skill_name);
                        enabled_skills.push(skill_name);
                    }
                }
            }
        } else {
            tracing::info!("技能自动激活已禁用");
        }
        
        let request_with_skills = ChatRequest {
            messages: request.messages.clone(),
            enabled_tools: request.enabled_tools.clone(),
            enabled_skills,
            auto_skills: request.auto_skills,
            session_id: request.session_id.clone(),
        };
        
        // 构建完整的系统提示（包含工作空间内容）
        let system_prompt = self.build_system_prompt(&request_with_skills);

        // 从配置或环境变量获取 API key
        let env_key = std::env::var("JIACLAW_API_KEY").ok();
        let api_key = self
            .config
            .provider
            .api_key
            .as_deref()
            .or(env_key.as_deref());

        // 根据提供商类型执行工具循环
        let provider_type = self.config.provider.provider_type.as_str();
        match (api_key, provider_type) {
            (Some(key), "brokerrouter" | "openai_compatible") => {
                // 使用工具执行循环
                self.execute_tool_loop(
                    request_with_skills.messages.clone(),
                    &system_prompt,
                    provider_type,
                    Some(key),
                )
                .await
            }
            (Some(_), unknown_type) => {
                tracing::warn!(
                    "未知的提供商类型 '{}', 回退到存根模式",
                    unknown_type
                );
                // 存根模式也支持工具执行
                self.execute_tool_loop(
                    request_with_skills.messages.clone(),
                    &system_prompt,
                    "stub",
                    None,
                )
                .await
            }
            (None, _) => {
                // 存根模式也支持工具执行
                tracing::warn!(
                    "未配置 API key（通过配置文件或 JIACLAW_API_KEY 环境变量），使用存根模式"
                );
                self.execute_tool_loop(
                    request_with_skills.messages.clone(),
                    &system_prompt,
                    "stub",
                    None,
                )
                .await
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
        if !self.skills.is_empty() {
            prompt.push_str("\n\n## Available Skills\n\n");
            for skill in &self.skills {
                prompt.push_str(&format!("- {}\n", skill.summary()));
            }

            // 如果用户请求了特定技能，添加详细信息
            if !request.enabled_skills.is_empty() {
                prompt.push_str("\n### Enabled Skills (详细)\n");
                for skill_name in &request.enabled_skills {
                    if let Some(skill) = self.skills.iter().find(|s| &s.name == skill_name) {
                        prompt.push_str(&format!("\n#### {}\n", skill.name));
                        prompt.push_str(&skill.content);
                        prompt.push('\n');
                    }
                }
            }
        }

        // 添加可用工具列表
        let tool_list = self.tools.list();
        if !tool_list.is_empty() {
            prompt.push_str("\n\n## Available Tools\n\n");
            prompt.push_str("你可以使用以下工具来完成任务。每个工具的详细信息如下：\n\n");
            for tool_name in tool_list {
                if let Some(tool) = self.tools.get(tool_name) {
                    prompt.push_str(&format!(
                        "### {}\n{}\n\n参数 schema:\n```json\n{}\n```\n\n",
                        tool.name(),
                        tool.description(),
                        serde_json::to_string_pretty(&tool.parameters_schema()).unwrap_or_default()
                    ));
                }
            }
            prompt.push_str(
                "## Tool Calling Format\n\n\
                 要调用工具，请在你的响应中使用以下 JSON 代码块格式：\n\n\
                 ```tool\n\
                 {\n\
                   \"tool_name\": \"工具名称\",\n\
                   \"arguments\": {\"参数名\": \"参数值\"}\n\
                 }\n\
                 ```\n\n\
                 你可以在一条消息中调用多个工具，每个工具调用使用一个单独的 ```tool 代码块。\n\
                 我会执行这些工具并将结果返回给你，然后你可以继续处理。\n\n"
            );
        }

        prompt
    }

    /// 解析 assistant 消息中的工具调用
    fn parse_tool_calls(content: &str) -> Vec<ToolCall> {
        let mut tool_calls = Vec::new();
        
        // 查找所有 ```tool ... ``` 代码块
        let mut start_idx = 0;
        while let Some(block_start) = content[start_idx..].find("```tool") {
            let block_start = start_idx + block_start;
            if let Some(block_end) = content[block_start + 7..].find("```") {
                let block_end = block_start + 7 + block_end;
                let json_str = content[block_start + 7..block_end].trim();
                
                // 尝试解析 JSON
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                    if let (Some(tool_name), Some(arguments)) = (
                        parsed.get("tool_name").and_then(|v| v.as_str()),
                        parsed.get("arguments"),
                    ) {
                        tool_calls.push(ToolCall {
                            tool_name: tool_name.to_string(),
                            arguments: arguments.clone(),
                            result: None,
                        });
                    }
                }
                
                start_idx = block_end + 3;
            } else {
                break;
            }
        }
        
        tool_calls
    }

    /// 执行工具调用循环
    async fn execute_tool_loop(
        &self,
        mut messages: Vec<ChatMessage>,
        system_prompt: &str,
        provider_type: &str,
        api_key: Option<&str>,
    ) -> Result<ChatResponse, JiaClawError> {
        const MAX_ITERATIONS: usize = 5;
        let mut iteration = 0;
        let mut all_tool_calls = Vec::new();
        
        loop {
            iteration += 1;
            
            // 调用 LLM
            let response = if let Some(key) = api_key {
                match provider_type {
                    "brokerrouter" => {
                        let provider = BrokerrouterProvider::new(&self.config.provider.base_url, key);
                        provider
                            .chat(
                                &self.config.provider.model,
                                system_prompt,
                                &messages,
                                self.config.provider.temperature,
                                self.config.provider.max_tokens,
                            )
                            .await?
                    }
                    "openai_compatible" => {
                        let provider = OpenAICompatibleProvider::new(&self.config.provider.base_url, key);
                        provider
                            .chat(
                                &self.config.provider.model,
                                system_prompt,
                                &messages,
                                self.config.provider.temperature,
                                self.config.provider.max_tokens,
                            )
                            .await?
                    }
                    _ => {
                        return Err(JiaClawError::Configuration(format!(
                            "未知的提供商类型: {provider_type}"
                        )));
                    }
                }
            } else {
                // 存根模式
                let request = ChatRequest {
                    messages: messages.clone(),
                    enabled_tools: vec![],
                    enabled_skills: vec![],
                    auto_skills: true,
                    session_id: None,
                };
                self.stub_chat(&request, system_prompt)
            };
            
            // 解析工具调用
            let tool_calls = Self::parse_tool_calls(&response.message.content);
            
            if tool_calls.is_empty() || iteration >= MAX_ITERATIONS {
                // 没有工具调用或达到最大迭代次数，返回结果（包含所有已执行的工具调用）
                return Ok(ChatResponse {
                    message: response.message,
                    tool_calls: all_tool_calls,
                    status: response.status,
                    session_id: None,
                });
            }
            
            // 执行工具调用
            let mut executed_tool_calls = Vec::new();
            let mut tool_results = Vec::new();
            
            for mut tool_call in tool_calls {
                tracing::info!("执行工具: {} (迭代 {}/{})", tool_call.tool_name, iteration, MAX_ITERATIONS);
                
                match self.tools.execute(&tool_call).await {
                    Ok(result) => {
                        tool_call.result = Some(serde_json::json!(result.clone()));
                        executed_tool_calls.push(tool_call.clone());
                        tool_results.push(format!(
                            "工具 {} 执行成功:\n{}",
                            tool_call.tool_name,
                            result
                        ));
                    }
                    Err(e) => {
                        let error_msg = format!("工具 {} 执行失败: {}", tool_call.tool_name, e);
                        tool_call.result = Some(serde_json::json!({"error": error_msg.clone()}));
                        executed_tool_calls.push(tool_call.clone());
                        tool_results.push(error_msg);
                    }
                }
            }
            
            // 将所有执行的工具调用添加到累积列表
            all_tool_calls.extend(executed_tool_calls);
            
            // 将 assistant 的响应和工具结果添加到历史
            messages.push(response.message.clone());
            messages.push(ChatMessage {
                role: MessageRole::User,
                content: format!(
                    "工具执行结果 (迭代 {}/{}):\n\n{}",
                    iteration,
                    MAX_ITERATIONS,
                    tool_results.join("\n\n")
                ),
            });
        }
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
            session_id: None,
        }
    }

    /// 生成存根响应（演示用）
    #[allow(clippy::too_many_lines)]
    fn generate_stub_response(
        &self,
        user_message: &str,
        request: &ChatRequest,
        _system_prompt: &str,
    ) -> String {
        let user_lower = user_message.to_lowercase();

        // 检查是否是工具执行结果反馈
        if user_lower.contains("工具执行结果") || user_lower.contains("执行成功") {
            // 工具已经执行完成，返回一个总结
            return "根据工具执行结果，操作已完成。\n\n\
                 如需了解更多信息，请查看上面的工具输出结果。".to_string();
        }

        // 检查是否应该触发工具调用（演示）
        if user_lower.contains("列出工作空间") 
            || user_lower.contains("list workspace")
            || user_lower.contains("workspace files") {
            return "好的,让我列出工作空间文件。\n\n\
                 ```tool\n\
                 {\n\
                   \"tool_name\": \"workspace_list\",\n\
                   \"arguments\": {}\n\
                 }\n\
                 ```".to_string();
        }
        
        if user_lower.contains("读取记忆") 
            || user_lower.contains("read memory")
            || (user_lower.contains("memory") && user_lower.contains("read")) {
            return "让我读取记忆文件。\n\n\
                 ```tool\n\
                 {\n\
                   \"tool_name\": \"memory_read\",\n\
                   \"arguments\": {\"file\": \"MEMORY\"}\n\
                 }\n\
                 ```".to_string();
        }
        
        if user_lower.contains("当前时间") 
            || user_lower.contains("current time")
            || user_lower.contains("what time") {
            return "让我获取当前时间。\n\n\
                 ```tool\n\
                 {\n\
                   \"tool_name\": \"datetime_now\",\n\
                   \"arguments\": {}\n\
                 }\n\
                 ```".to_string();
        }
        
        if (user_lower.contains("列出") || user_lower.contains("list")) 
            && user_lower.contains("文件") {
            return "让我列出当前目录的文件。\n\n\
                 ```tool\n\
                 {\n\
                   \"tool_name\": \"file_list\",\n\
                   \"arguments\": {\"path\": \".\"}\n\
                 }\n\
                 ```".to_string();
        }

        // 简单的关键词匹配响应
        if user_lower.contains("你好") || user_lower.contains("hello") || user_lower.contains("hi")
        {
            format!(
                "你好！我是 {}。{}\n\n\
                 我目前运行在存根模式下，等待 StateKnot 框架集成。\n\
                 当前配置的最大对话轮次为 {} 轮。\n\n\
                 💡 可用功能（演示）：\n\
                 • 工作空间已加载（{} 个文件）\n\
                 • 发现了 {} 个技能\n\
                 • 注册了 {} 个工具\n\n\
                 试试问我：\"列出工作空间\" 或 \"读取记忆\" 来测试工具执行！",
                self.config.name,
                self.config.description,
                self.config.max_turns,
                self.count_workspace_files(),
                self.skills.len(),
                self.tools.list().len()
            )
        } else if user_lower.contains("工具") || user_lower.contains("tool") {
            let tool_list = self.tools.list();
            if tool_list.is_empty() {
                "目前没有注册任何工具。".to_string()
            } else {
                let mut response = format!("🔧 已注册 {} 个本地工具：\n\n", tool_list.len());
                for tool_name in tool_list {
                    if let Some(tool) = self.tools.get(tool_name) {
                        response.push_str(&format!(
                            "• **{}**\n  {}\n\n",
                            tool.name(),
                            tool.description()
                        ));
                    }
                }
                response.push_str("💡 试试说：\n");
                response.push_str("• \"列出工作空间\" - 调用 workspace_list 工具\n");
                response.push_str("• \"读取记忆\" - 调用 memory_read 工具\n");
                response.push_str("• \"当前时间\" - 调用 datetime_now 工具\n");
                response
            }
        } else if user_lower.contains("技能") || user_lower.contains("skill") {
            if self.skills.is_empty() {
                "目前没有发现任何技能。\n\n运行 'jiaclaw init' 会创建示例技能。".to_string()
            } else {
                let mut response = format!("🎯 发现 {} 个技能：\n\n", self.skills.len());
                for skill in &self.skills {
                    response.push_str(&format!("• {}\n\n", skill.summary()));
                }
                response.push_str("完整的技能系统将在 M3 实现。");
                response
            }
        } else if user_lower.contains("功能")
            || user_lower.contains("能力")
            || user_lower.contains("what can you do")
        {
            let tools_str = if request.enabled_tools.is_empty() {
                format!("{} 个本地工具", self.tools.list().len())
            } else {
                request.enabled_tools.join(", ")
            };
            let skills_str = if self.skills.is_empty() {
                "无".to_string()
            } else {
                format!("{} 个技能", self.skills.len())
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
                 • 已注册工具：{}\n\
                 • 已发现技能：{}\n\
                 • 工作空间文件：{} 个已加载\n\n\
                 查看文档了解更多：docs/architecture.md",
                self.config.name,
                tools_str,
                skills_str,
                self.count_workspace_files()
            )
        } else if user_lower.contains("帮助") || user_lower.contains("help") {
            "📖 JiaClaw 帮助\n\n\
             命令:\n\
             • `jiaclaw init` - 初始化工作空间\n\
             • `jiaclaw version` - 显示版本信息\n\
             • `jiaclaw chat <消息>` - 发送单次聊天消息\n\
             • `jiaclaw doctor` - 检查配置和连接\n\
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
                 • 工作空间文件: {} 个\n\
                 • 发现技能: {} 个\n\
                 • 注册工具: {} 个\n\n\
                 系统状态:\n\
                 • 运行模式: 存根（Stub）\n\
                 • StateKnot 集成: 等待中\n\
                 • 持久化: 未启用\n\n\
                 💡 提示：运行 'jiaclaw doctor' 检查配置",
                self.config.name,
                self.config.name,
                self.config.description,
                self.config.max_turns,
                request.messages.len(),
                self.count_workspace_files(),
                self.skills.len(),
                self.tools.list().len()
            )
        } else if user_message.trim().is_empty() {
            "请发送一条消息开始对话。你可以说\"你好\"或询问\"你有什么功能\"。".to_string()
        } else {
            // 默认响应
            let workspace_hint = if self.count_workspace_files() > 0 {
                format!(
                    "\n\n💡 我已加载了你的工作空间配置（{} 个文件），包括你的偏好和记忆。",
                    self.count_workspace_files()
                )
            } else {
                "\n\n💡 运行 'jiaclaw init' 创建工作空间以个性化我的行为。".to_string()
            };

            format!(
                "我收到了你的消息：\"{user_message}\"\n\n\
                 目前我运行在存根模式下，无法进行实际的自然语言理解或生成。\
                 当 StateKnot 框架集成完成后，我将能够：\n\
                 • 理解复杂的自然语言输入\n\
                 • 使用模型生成智能回复\n\
                 • 调用工具完成实际任务\n\
                 • 保持持久化的对话上下文{workspace_hint}\n\n\
                 试试说\"帮助\"了解更多命令。"
            )
        }
    }

    /// 统计工作空间文件数量
    fn count_workspace_files(&self) -> usize {
        let mut count = 0;
        if self.workspace.agents.is_some() {
            count += 1;
        }
        if self.workspace.soul.is_some() {
            count += 1;
        }
        if self.workspace.user.is_some() {
            count += 1;
        }
        if self.workspace.memory.is_some() {
            count += 1;
        }
        count
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
            auto_skills: true,
            session_id: None,
        };

        let response = agent.chat(&request).await;
        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn test_tool_execution_in_stub_mode() {
        let config = AgentConfig::default();
        let agent = JiaClawAgent::new(config).unwrap();

        // 测试工具调用触发
        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "列出工作空间".to_string(),
            }],
            enabled_tools: vec![],
            enabled_skills: vec![],
            auto_skills: true,
            session_id: None,
        };

        let response = agent.chat(&request).await;
        assert!(response.is_ok());
        let response = response.unwrap();
        
        // 验证工具被执行了
        assert!(
            !response.tool_calls.is_empty(),
            "工具应该被执行，tool_calls 不应为空"
        );
        assert_eq!(response.tool_calls[0].tool_name, "workspace_list");
        assert!(response.tool_calls[0].result.is_some(), "工具应该有执行结果");
    }

    #[tokio::test]
    async fn test_tool_call_parsing() {
        // 测试工具调用解析
        let content = r#"
让我列出工作空间文件。

```tool
{
  "tool_name": "workspace_list",
  "arguments": {}
}
```
"#;

        let tool_calls = JiaClawAgent::parse_tool_calls(content);
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].tool_name, "workspace_list");
    }

    #[tokio::test]
    async fn test_multiple_tool_calls_parsing() {
        // 测试多个工具调用解析
        let content = r#"
让我先获取时间，然后列出文件。

```tool
{
  "tool_name": "datetime_now",
  "arguments": {}
}
```

然后

```tool
{
  "tool_name": "workspace_list",
  "arguments": {}
}
```
"#;

        let tool_calls = JiaClawAgent::parse_tool_calls(content);
        assert_eq!(tool_calls.len(), 2);
        assert_eq!(tool_calls[0].tool_name, "datetime_now");
        assert_eq!(tool_calls[1].tool_name, "workspace_list");
    }

    #[tokio::test]
    async fn test_auto_skills_disabled() {
        // 测试禁用自动技能激活
        let config = AgentConfig::default();
        let agent = JiaClawAgent::new(config).unwrap();

        // 假设有一个技能会被 "搜索" 触发
        // 但我们设置 auto_skills = false
        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "搜索 OpenAI".to_string(),
            }],
            enabled_tools: vec![],
            enabled_skills: vec![],
            auto_skills: false,
            session_id: None,
        };

        let response = agent.chat(&request).await;
        assert!(response.is_ok());
        
        // 即使内容可能触发技能，也不应该自动启用
        // （因为默认工作空间没有技能，这个测试主要验证不会崩溃）
    }

    #[tokio::test]
    async fn test_auto_skills_enabled() {
        // 测试启用自动技能激活
        let config = AgentConfig::default();
        let agent = JiaClawAgent::new(config).unwrap();

        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "搜索信息".to_string(),
            }],
            enabled_tools: vec![],
            enabled_skills: vec![],
            auto_skills: true,
            session_id: None,
        };

        let response = agent.chat(&request).await;
        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn test_explicit_skills_with_auto_disabled() {
        // 测试显式指定技能 + 禁用自动激活
        // 显式技能应该仍然生效
        let config = AgentConfig::default();
        let agent = JiaClawAgent::new(config).unwrap();

        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "你好".to_string(),
            }],
            enabled_tools: vec![],
            enabled_skills: vec!["calculator".to_string()],
            auto_skills: false,
            session_id: None,
        };

        let response = agent.chat(&request).await;
        assert!(response.is_ok());
        // 显式指定的技能应该在系统提示中
    }
}
