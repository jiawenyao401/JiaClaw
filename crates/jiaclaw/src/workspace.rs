// Copyright 2026 JiaClaw contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! 工作空间管理

use jiaclaw_core::JiaClawError;
use std::path::{Path, PathBuf};

/// 工作空间文件
#[derive(Debug, Clone)]
pub struct Workspace {
    /// 工作空间路径
    pub path: PathBuf,

    /// AGENTS.md - Agent 配置和元数据
    pub agents: Option<String>,

    /// SOUL.md - Agent 性格和指令
    pub soul: Option<String>,

    /// USER.md - 用户信息和偏好
    pub user: Option<String>,

    /// MEMORY.md - 长期记忆和上下文
    pub memory: Option<String>,
}

impl Workspace {
    /// 加载工作空间文件
    ///
    /// # Errors
    ///
    /// 当前实现总是返回 `Ok`，即使文件不存在（返回 None 字段）。
    pub fn load(path: &Path) -> Result<Self, JiaClawError> {
        let agents = Self::load_file(path, "AGENTS.md").ok();
        let soul = Self::load_file(path, "SOUL.md").ok();
        let user = Self::load_file(path, "USER.md").ok();
        let memory = Self::load_file(path, "MEMORY.md").ok();

        Ok(Self {
            path: path.to_path_buf(),
            agents,
            soul,
            user,
            memory,
        })
    }

    /// 初始化工作空间（创建默认文件）
    ///
    /// # Errors
    ///
    /// 如果无法创建目录或写入文件，返回错误。
    pub fn init(path: &Path) -> Result<Self, JiaClawError> {
        // 创建工作空间目录
        std::fs::create_dir_all(path)
            .map_err(|e| JiaClawError::Configuration(format!("无法创建工作空间目录: {e}")))?;

        // 创建默认文件
        Self::write_file(path, "AGENTS.md", Self::default_agents_content())?;
        Self::write_file(path, "SOUL.md", Self::default_soul_content())?;
        Self::write_file(path, "USER.md", Self::default_user_content())?;
        Self::write_file(path, "MEMORY.md", Self::default_memory_content())?;

        // 创建 skills 目录
        let skills_path = path.join("skills");
        std::fs::create_dir_all(&skills_path)
            .map_err(|e| JiaClawError::Configuration(format!("无法创建 skills 目录: {e}")))?;

        // 创建示例技能
        Self::create_example_skill(&skills_path, "search")?;
        Self::create_example_skill(&skills_path, "calculator")?;

        Self::load(path)
    }

    /// 加载单个文件
    fn load_file(workspace_path: &Path, filename: &str) -> Result<String, JiaClawError> {
        let file_path = workspace_path.join(filename);
        if !file_path.exists() {
            return Err(JiaClawError::Configuration(format!(
                "文件不存在: {}",
                file_path.display()
            )));
        }

        std::fs::read_to_string(&file_path)
            .map_err(|e| JiaClawError::Configuration(format!("无法读取文件 {filename}: {e}")))
    }

    /// 写入文件
    fn write_file(
        workspace_path: &Path,
        filename: &str,
        content: &str,
    ) -> Result<(), JiaClawError> {
        let file_path = workspace_path.join(filename);
        std::fs::write(&file_path, content)
            .map_err(|e| JiaClawError::Configuration(format!("无法写入文件 {filename}: {e}")))
    }

    /// 创建示例技能
    fn create_example_skill(skills_path: &Path, skill_name: &str) -> Result<(), JiaClawError> {
        let skill_dir = skills_path.join(skill_name);
        std::fs::create_dir_all(&skill_dir).map_err(|e| {
            JiaClawError::Configuration(format!("无法创建技能目录 {skill_name}: {e}"))
        })?;

        let content = match skill_name {
            "search" => Self::search_skill_content(),
            "calculator" => Self::calculator_skill_content(),
            _ => return Ok(()),
        };

        let skill_file = skill_dir.join("SKILL.md");
        std::fs::write(&skill_file, content).map_err(|e| {
            JiaClawError::Configuration(format!("无法写入技能文件 {skill_name}: {e}"))
        })?;

        Ok(())
    }

    // 默认内容模板

    fn default_agents_content() -> &'static str {
        r"# JiaClaw Agents

This file describes the agents in your workspace.

## Primary Agent

**Name**: JiaClaw  
**Type**: Personal Assistant  
**Description**: A helpful personal assistant for daily tasks and knowledge work.

## Capabilities

- Natural language conversation
- Tool usage (when configured)
- Skill-based task execution
- Long-term memory (when configured)

## Configuration

See `config/jiaclaw.toml` for detailed configuration options.
"
    }

    fn default_soul_content() -> &'static str {
        r"# Agent Soul

This file defines the personality and behavioral characteristics of your JiaClaw agent.

## Personality Traits

- **Helpful**: Always eager to assist with tasks
- **Patient**: Takes time to understand user needs
- **Concise**: Provides clear, direct answers
- **Honest**: Admits limitations and uncertainties

## Communication Style

- Use simple, clear language
- Ask clarifying questions when needed
- Provide step-by-step explanations for complex topics
- Be proactive in suggesting helpful actions

## Values

- Privacy: Never share user information externally
- Accuracy: Prioritize correctness over speed
- Transparency: Explain reasoning and sources
"
    }

    fn default_user_content() -> &'static str {
        r"# User Profile

This file contains information about you, the user, to help JiaClaw provide personalized assistance.

## About You

**Name**: [Your Name]  
**Role**: [Your Role/Occupation]  
**Timezone**: [Your Timezone]  

## Preferences

- **Communication**: [e.g., formal/casual, concise/detailed]
- **Language**: [Primary language(s)]
- **Working Hours**: [e.g., 9 AM - 5 PM]

## Common Tasks

List your frequently performed tasks here:

- [Task 1]
- [Task 2]
- [Task 3]

## Background Context

Add any relevant background information that helps JiaClaw understand your needs better.
"
    }

    fn default_memory_content() -> &'static str {
        r"# Long-term Memory

This file stores important context and information from past conversations.

## Key Facts

- [Important fact 1]
- [Important fact 2]

## Ongoing Projects

### [Project Name]

- **Status**: [In Progress/Completed]
- **Description**: [Brief description]
- **Next Steps**: [What to do next]

## Learned Preferences

- [Preference 1]
- [Preference 2]

---

*Note: This file can be manually edited or will be updated automatically when StateKnot persistence is enabled.*
"
    }

    fn search_skill_content() -> &'static str {
        r#"---
name: search
description: Web search capability for finding information online
triggers:
  - search
  - 搜索
  - find
  - 查找
  - look up
---

# Search Skill

This skill provides web search capabilities for finding information, news, and current events.

## Tools

- `web_search` - Search the web using a search engine
- `fetch_url` - Fetch and extract content from a URL

## Usage

When the user asks to search for information, news, or current events, use this skill.

## Examples

- "Search for the latest news on AI"
- "Find information about Rust programming"
- "Look up the weather forecast"
- "搜索最新的 AI 新闻"

## Implementation Status

⏳ **Planned** - Awaiting tool system implementation (M2)

## Dependencies

- HTTP client for web requests
- Search engine API (e.g., DuckDuckGo, Google Custom Search)
"#
    }

    fn calculator_skill_content() -> &'static str {
        r#"---
name: calculator
description: Mathematical computation and evaluation skill
triggers:
  - calculate
  - 计算
  - math
  - 数学
  - compute
---

# Calculator Skill

This skill provides mathematical computation and evaluation capabilities.

## Tools

- `evaluate` - Evaluate mathematical expressions
- `convert_units` - Convert between units (length, weight, temperature, etc.)

## Usage

When the user asks to perform calculations or unit conversions, use this skill.

## Examples

- "Calculate 15% tip on $85"
- "Convert 100 kilometers to miles"
- "What's the square root of 144?"
- "计算 123 * 456"

## Implementation Status

⏳ **Planned** - Awaiting tool system implementation (M2)

## Dependencies

- Math expression parser
- Unit conversion library
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_workspace_init() {
        let temp_dir = std::env::temp_dir().join("jiaclaw_test_workspace");
        let _ = fs::remove_dir_all(&temp_dir);

        let workspace = Workspace::init(&temp_dir).unwrap();

        assert!(workspace.agents.is_some());
        assert!(workspace.soul.is_some());
        assert!(workspace.user.is_some());
        assert!(workspace.memory.is_some());

        // 验证文件存在
        assert!(temp_dir.join("AGENTS.md").exists());
        assert!(temp_dir.join("SOUL.md").exists());
        assert!(temp_dir.join("USER.md").exists());
        assert!(temp_dir.join("MEMORY.md").exists());
        assert!(temp_dir.join("skills").exists());

        // 清理
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_workspace_load_missing() {
        let temp_dir = std::env::temp_dir().join("jiaclaw_test_missing");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let workspace = Workspace::load(&temp_dir).unwrap();

        assert!(workspace.agents.is_none());
        assert!(workspace.soul.is_none());
        assert!(workspace.user.is_none());
        assert!(workspace.memory.is_none());

        // 清理
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
