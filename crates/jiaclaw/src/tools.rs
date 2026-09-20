// Copyright 2026 JiaClaw contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! 工具系统（本地工具实现）

use async_trait::async_trait;
use jiaclaw_core::{JiaClawError, ToolCall};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 工具 trait
#[async_trait]
pub trait Tool: Send + Sync {
    /// 工具名称
    fn name(&self) -> &str;

    /// 工具描述
    fn description(&self) -> &str;

    /// 工具参数 schema（JSON Schema 格式）
    fn parameters_schema(&self) -> Value;

    /// 执行工具
    async fn execute(&self, args: Value) -> Result<String, JiaClawError>;
}

/// 工具注册表
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    /// 创建新的工具注册表
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// 注册工具
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let name = tool.name().to_string();
        tracing::debug!("注册工具: {}", name);
        self.tools.insert(name, tool);
    }

    /// 获取工具
    pub fn get(&self, name: &str) -> Option<&Box<dyn Tool>> {
        self.tools.get(name)
    }

    /// 列出所有工具
    pub fn list(&self) -> Vec<&str> {
        self.tools.keys().map(String::as_str).collect()
    }

    /// 执行工具调用
    pub async fn execute(&self, tool_call: &ToolCall) -> Result<String, JiaClawError> {
        let tool = self.get(&tool_call.tool_name).ok_or_else(|| {
            JiaClawError::ToolExecution(format!("工具不存在: {}", tool_call.tool_name))
        })?;

        tool.execute(tool_call.arguments.clone()).await
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Workspace List 工具（列出工作空间文件）
pub struct WorkspaceListTool {
    workspace_path: PathBuf,
}

impl WorkspaceListTool {
    /// 创建新的工作空间列表工具
    pub fn new(workspace_path: &Path) -> Self {
        Self {
            workspace_path: workspace_path.to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for WorkspaceListTool {
    fn name(&self) -> &str {
        "workspace_list"
    }

    fn description(&self) -> &str {
        "列出工作空间中的文件和目录结构"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(&self, _args: Value) -> Result<String, JiaClawError> {
        if !self.workspace_path.exists() {
            return Ok(format!(
                "工作空间不存在: {}\n提示: 运行 'jiaclaw init' 创建工作空间",
                self.workspace_path.display()
            ));
        }

        let mut result = format!("工作空间: {}\n\n", self.workspace_path.display());

        // 列出工作空间根文件
        let root_files = vec!["AGENTS.md", "SOUL.md", "USER.md", "MEMORY.md"];
        result.push_str("文件:\n");

        for file in &root_files {
            let file_path = self.workspace_path.join(file);
            if file_path.exists() {
                let metadata = std::fs::metadata(&file_path)
                    .map_err(|e| JiaClawError::ToolExecution(format!("无法读取文件元数据: {e}")))?;
                result.push_str(&format!("  • {} ({} bytes)\n", file, metadata.len()));
            } else {
                result.push_str(&format!("  • {} (不存在)\n", file));
            }
        }

        // 列出技能目录
        let skills_dir = self.workspace_path.join("skills");
        if skills_dir.exists() {
            result.push_str("\n技能:\n");

            let entries = std::fs::read_dir(&skills_dir)
                .map_err(|e| JiaClawError::ToolExecution(format!("无法读取技能目录: {e}")))?;

            for entry in entries {
                let entry = entry
                    .map_err(|e| JiaClawError::ToolExecution(format!("无法读取目录条目: {e}")))?;

                if entry.path().is_dir() {
                    let skill_name = entry.file_name();
                    let skill_file = entry.path().join("SKILL.md");
                    if skill_file.exists() {
                        result.push_str(&format!("  • {}/\n", skill_name.to_string_lossy()));
                    }
                }
            }
        } else {
            result.push_str("\n技能目录不存在\n");
        }

        Ok(result)
    }
}

/// Memory Read 工具（读取记忆文件）
pub struct MemoryReadTool {
    workspace_path: PathBuf,
}

impl MemoryReadTool {
    /// 创建新的记忆读取工具
    pub fn new(workspace_path: &Path) -> Self {
        Self {
            workspace_path: workspace_path.to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for MemoryReadTool {
    fn name(&self) -> &str {
        "memory_read"
    }

    fn description(&self) -> &str {
        "读取长期记忆文件的内容"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file": {
                    "type": "string",
                    "enum": ["MEMORY.md", "USER.md", "SOUL.md", "AGENTS.md"],
                    "description": "要读取的文件名"
                }
            },
            "required": ["file"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String, JiaClawError> {
        let file_name = args
            .get("file")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JiaClawError::ToolExecution("缺少参数 'file'".to_string()))?;

        // 验证文件名
        let allowed_files = ["MEMORY.md", "USER.md", "SOUL.md", "AGENTS.md"];
        if !allowed_files.contains(&file_name) {
            return Err(JiaClawError::ToolExecution(format!(
                "不允许读取的文件: {}. 仅允许: {:?}",
                file_name, allowed_files
            )));
        }

        let file_path = self.workspace_path.join(file_name);

        if !file_path.exists() {
            return Ok(format!(
                "文件不存在: {}\n提示: 运行 'jiaclaw init' 创建工作空间",
                file_path.display()
            ));
        }

        let content = std::fs::read_to_string(&file_path)
            .map_err(|e| JiaClawError::ToolExecution(format!("无法读取文件: {e}")))?;

        Ok(format!(
            "文件: {}\n长度: {} 字节\n\n{}",
            file_name,
            content.len(),
            content
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn test_workspace_list_tool() {
        let temp_workspace = std::env::temp_dir().join("jiaclaw_test_tools_workspace");
        let _ = fs::remove_dir_all(&temp_workspace);
        fs::create_dir_all(&temp_workspace).unwrap();

        // 创建测试文件
        fs::write(temp_workspace.join("MEMORY.md"), "test memory").unwrap();
        fs::write(temp_workspace.join("USER.md"), "test user").unwrap();

        let tool = WorkspaceListTool::new(&temp_workspace);

        assert_eq!(tool.name(), "workspace_list");
        assert!(!tool.description().is_empty());

        let result = tool.execute(serde_json::json!({})).await.unwrap();

        assert!(result.contains("MEMORY.md"));
        assert!(result.contains("USER.md"));

        let _ = fs::remove_dir_all(&temp_workspace);
    }

    #[tokio::test]
    async fn test_memory_read_tool() {
        let temp_workspace = std::env::temp_dir().join("jiaclaw_test_tools_memory");
        let _ = fs::remove_dir_all(&temp_workspace);
        fs::create_dir_all(&temp_workspace).unwrap();

        let test_content = "# Test Memory\n\nThis is test content.";
        fs::write(temp_workspace.join("MEMORY.md"), test_content).unwrap();

        let tool = MemoryReadTool::new(&temp_workspace);

        assert_eq!(tool.name(), "memory_read");

        let result = tool
            .execute(serde_json::json!({"file": "MEMORY.md"}))
            .await
            .unwrap();

        assert!(result.contains(test_content));

        // 测试无效文件
        let result = tool
            .execute(serde_json::json!({"file": "invalid.txt"}))
            .await;
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&temp_workspace);
    }

    #[test]
    fn test_tool_registry() {
        let temp_workspace = std::env::temp_dir().join("jiaclaw_test_registry");

        let mut registry = ToolRegistry::new();

        let tool = Box::new(WorkspaceListTool::new(&temp_workspace));
        registry.register(tool);

        assert!(registry.get("workspace_list").is_some());
        assert!(registry.get("nonexistent").is_none());

        let tools = registry.list();
        assert!(tools.contains(&"workspace_list"));
    }
}
