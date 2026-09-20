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
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|b| &**b)
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
                result.push_str(&format!("  • {file} (不存在)\n"));
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
                "不允许读取的文件: {file_name}. 仅允许: {allowed_files:?}"
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

/// File Read 工具（读取工作空间文件）
pub struct FileReadTool {
    workspace_path: PathBuf,
}

impl FileReadTool {
    /// 创建新的文件读取工具
    pub fn new(workspace_path: &Path) -> Self {
        // 规范化工作空间路径，确保沙箱检查在所有平台上一致
        let canonical_workspace = workspace_path
            .canonicalize()
            .unwrap_or_else(|_| workspace_path.to_path_buf());
        
        Self {
            workspace_path: canonical_workspace,
        }
    }
}

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }

    fn description(&self) -> &str {
        "读取工作空间中的文件内容（相对路径，沙箱化到工作空间）"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "文件相对路径（相对于工作空间根目录）"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String, JiaClawError> {
        let relative_path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JiaClawError::ToolExecution("缺少参数 'path'".to_string()))?;

        // 规范化路径以防止目录遍历攻击
        let file_path = self.workspace_path.join(relative_path);
        let canonical_path = file_path.canonicalize().unwrap_or(file_path.clone());

        // 确保文件在工作空间内
        if !canonical_path.starts_with(&self.workspace_path) {
            return Err(JiaClawError::ToolExecution(format!(
                "安全错误: 文件 {relative_path} 在工作空间外部"
            )));
        }

        if !canonical_path.exists() {
            return Ok(format!("文件不存在: {relative_path}"));
        }

        if !canonical_path.is_file() {
            return Err(JiaClawError::ToolExecution(format!(
                "路径 {relative_path} 不是文件"
            )));
        }

        let content = std::fs::read_to_string(&canonical_path)
            .map_err(|e| JiaClawError::ToolExecution(format!("无法读取文件: {e}")))?;

        Ok(format!(
            "文件: {}\n路径: {}\n大小: {} 字节\n\n{}",
            relative_path,
            canonical_path.display(),
            content.len(),
            content
        ))
    }
}

/// File Write 工具（写入工作空间文件）
pub struct FileWriteTool {
    workspace_path: PathBuf,
}

impl FileWriteTool {
    /// 创建新的文件写入工具
    pub fn new(workspace_path: &Path) -> Self {
        // 规范化工作空间路径，确保沙箱检查在所有平台上一致
        let canonical_workspace = workspace_path
            .canonicalize()
            .unwrap_or_else(|_| workspace_path.to_path_buf());
        
        Self {
            workspace_path: canonical_workspace,
        }
    }
}

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "file_write"
    }

    fn description(&self) -> &str {
        "在工作空间中写入或创建文件（相对路径，沙箱化到工作空间）"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "文件相对路径（相对于工作空间根目录）"
                },
                "content": {
                    "type": "string",
                    "description": "要写入的文件内容"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String, JiaClawError> {
        let relative_path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JiaClawError::ToolExecution("缺少参数 'path'".to_string()))?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JiaClawError::ToolExecution("缺少参数 'content'".to_string()))?;

        // 构建文件路径
        let file_path = self.workspace_path.join(relative_path);

        // 确保目标路径在工作空间内（before creating file)
        let parent = file_path
            .parent()
            .ok_or_else(|| JiaClawError::ToolExecution("无效的文件路径".to_string()))?;

        // 规范化父目录路径（如果存在）进行安全检查
        let canonical_parent = if parent.exists() {
            parent.canonicalize().unwrap_or_else(|_| parent.to_path_buf())
        } else {
            parent.to_path_buf()
        };
        
        if !canonical_parent.starts_with(&self.workspace_path) {
            return Err(JiaClawError::ToolExecution(format!(
                "安全错误: 文件 {relative_path} 在工作空间外部"
            )));
        }

        // 创建父目录（如果不存在）
        std::fs::create_dir_all(parent)
            .map_err(|e| JiaClawError::ToolExecution(format!("无法创建目录: {e}")))?;

        // 写入文件
        std::fs::write(&file_path, content)
            .map_err(|e| JiaClawError::ToolExecution(format!("无法写入文件: {e}")))?;

        Ok(format!(
            "✅ 文件已写入: {}\n路径: {}\n大小: {} 字节",
            relative_path,
            file_path.display(),
            content.len()
        ))
    }
}

/// HTTP GET 工具
pub struct HttpGetTool;

impl HttpGetTool {
    /// 创建新的HTTP GET工具
    pub fn new() -> Self {
        Self
    }
}

impl Default for HttpGetTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for HttpGetTool {
    fn name(&self) -> &str {
        "http_get"
    }

    fn description(&self) -> &str {
        "发送HTTP GET请求并返回响应内容"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "目标URL（必须是http或https）"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String, JiaClawError> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JiaClawError::ToolExecution("缺少参数 'url'".to_string()))?;

        // 验证URL格式
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(JiaClawError::ToolExecution(
                "URL必须以http://或https://开头".to_string(),
            ));
        }

        // 使用spawn_blocking执行同步HTTP请求
        let url_owned = url.to_string();
        tokio::task::spawn_blocking(move || {
            let response = minreq::get(&url_owned)
                .send()
                .map_err(|e| JiaClawError::ToolExecution(format!("HTTP请求失败: {e}")))?;

            let status = response.status_code;
            let body = response.as_str().unwrap_or_default().to_string();

            Ok(format!(
                "HTTP GET {}\n状态码: {}\n响应大小: {} 字节\n\n{}",
                url_owned,
                status,
                body.len(),
                if body.len() > 1000 {
                    format!(
                        "{}...\n\n[响应过长，已截断。完整大小: {} 字节]",
                        &body[..1000],
                        body.len()
                    )
                } else {
                    body
                }
            ))
        })
        .await
        .map_err(|e| JiaClawError::ToolExecution(format!("任务执行失败: {e}")))?
    }
}

/// DateTime 工具
pub struct DateTimeTool;

impl DateTimeTool {
    /// 创建新的DateTime工具
    pub fn new() -> Self {
        Self
    }
}

impl Default for DateTimeTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for DateTimeTool {
    fn name(&self) -> &str {
        "datetime_now"
    }

    fn description(&self) -> &str {
        "获取当前日期和时间（UTC）"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(&self, _args: Value) -> Result<String, JiaClawError> {
        use std::time::SystemTime;

        let now = SystemTime::now();
        let duration_since_epoch = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| JiaClawError::ToolExecution(format!("系统时间错误: {e}")))?;

        let timestamp = duration_since_epoch.as_secs();

        // 简单的UTC时间格式化
        let days_since_epoch = timestamp / 86400;
        let seconds_today = timestamp % 86400;
        let hours = seconds_today / 3600;
        let minutes = (seconds_today % 3600) / 60;
        let seconds = seconds_today % 60;

        // 简单的日期计算（从1970-01-01开始）
        let year = 1970 + days_since_epoch / 365; // 简化计算

        Ok(format!(
            "当前时间 (UTC):\n\
             Unix时间戳: {timestamp}\n\
             大约时间: {year}-??-?? {hours:02}:{minutes:02}:{seconds:02}\n\n\
             注意: 这是简化的时间表示。完整的日期时间功能将在后续迭代中添加。"
        ))
    }
}

/// JSON Query 工具
pub struct JsonQueryTool;

impl JsonQueryTool {
    /// 创建新的JSON查询工具
    pub fn new() -> Self {
        Self
    }
}

impl Default for JsonQueryTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for JsonQueryTool {
    fn name(&self) -> &str {
        "json_query"
    }

    fn description(&self) -> &str {
        "解析JSON字符串并提取指定路径的值（使用点号表示法，如：user.name）"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "json": {
                    "type": "string",
                    "description": "JSON字符串"
                },
                "path": {
                    "type": "string",
                    "description": "查询路径（可选，留空返回整个JSON）"
                }
            },
            "required": ["json"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String, JiaClawError> {
        let json_str = args
            .get("json")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JiaClawError::ToolExecution("缺少参数 'json'".to_string()))?;

        let path = args.get("path").and_then(|v| v.as_str());

        // 解析JSON
        let json_value: Value = serde_json::from_str(json_str)
            .map_err(|e| JiaClawError::ToolExecution(format!("JSON解析失败: {e}")))?;

        // 如果没有指定路径，返回格式化的整个JSON
        if path.is_none() || path == Some("") {
            let pretty = serde_json::to_string_pretty(&json_value)
                .unwrap_or_else(|_| json_value.to_string());
            return Ok(format!("JSON (格式化):\n\n{pretty}"));
        }

        // 处理路径查询
        let path = path.unwrap();
        let parts: Vec<&str> = path.split('.').collect();

        let mut current = &json_value;
        for part in &parts {
            current = current
                .get(part)
                .ok_or_else(|| JiaClawError::ToolExecution(format!("路径 '{path}' 不存在")))?;
        }

        let result = serde_json::to_string_pretty(current).unwrap_or_else(|_| current.to_string());

        Ok(format!("查询路径: {path}\n结果:\n\n{result}"))
    }
}

/// File List 工具（列出目录内容）
pub struct FileListTool {
    workspace_path: PathBuf,
}

impl FileListTool {
    /// 创建新的文件列表工具
    pub fn new(workspace_path: &Path) -> Self {
        // 规范化工作空间路径，确保沙箱检查在所有平台上一致
        let canonical_workspace = workspace_path
            .canonicalize()
            .unwrap_or_else(|_| workspace_path.to_path_buf());
        
        Self {
            workspace_path: canonical_workspace,
        }
    }
}

#[async_trait]
impl Tool for FileListTool {
    fn name(&self) -> &str {
        "file_list"
    }

    fn description(&self) -> &str {
        "列出工作空间目录中的文件和子目录"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "目录相对路径（可选，默认为根目录）"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: Value) -> Result<String, JiaClawError> {
        let relative_path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");

        let dir_path = if relative_path.is_empty() {
            self.workspace_path.clone()
        } else {
            self.workspace_path.join(relative_path)
        };

        // 安全检查
        let canonical_path = dir_path.canonicalize().unwrap_or(dir_path.clone());

        if !canonical_path.starts_with(&self.workspace_path) {
            return Err(JiaClawError::ToolExecution(format!(
                "安全错误: 路径 {relative_path} 在工作空间外部"
            )));
        }

        if !canonical_path.exists() {
            return Ok(format!("目录不存在: {relative_path}"));
        }

        if !canonical_path.is_dir() {
            return Err(JiaClawError::ToolExecution(format!(
                "路径 {relative_path} 不是目录"
            )));
        }

        let entries = std::fs::read_dir(&canonical_path)
            .map_err(|e| JiaClawError::ToolExecution(format!("无法读取目录: {e}")))?;

        let mut result = format!(
            "目录: {}\n\n",
            if relative_path.is_empty() {
                "/"
            } else {
                relative_path
            }
        );

        let mut files = Vec::new();
        let mut dirs = Vec::new();

        for entry in entries {
            let entry =
                entry.map_err(|e| JiaClawError::ToolExecution(format!("无法读取目录条目: {e}")))?;

            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if path.is_dir() {
                dirs.push(name);
            } else {
                let metadata = entry.metadata().ok();
                let size = metadata.map_or(0, |m| m.len());
                files.push((name, size));
            }
        }

        // 排序
        dirs.sort();
        files.sort_by(|a, b| a.0.cmp(&b.0));

        // 输出目录
        if !dirs.is_empty() {
            result.push_str("📁 目录:\n");
            for dir in &dirs {
                result.push_str(&format!("  {dir}/\n"));
            }
            result.push('\n');
        }

        // 输出文件
        if !files.is_empty() {
            result.push_str("📄 文件:\n");
            for (name, size) in &files {
                result.push_str(&format!("  {name} ({size} bytes)\n"));
            }
        }

        if dirs.is_empty() && files.is_empty() {
            result.push_str("(空目录)\n");
        }

        Ok(result)
    }
}

/// File Delete 工具（删除文件）
pub struct FileDeleteTool {
    workspace_path: PathBuf,
}

impl FileDeleteTool {
    /// 创建新的文件删除工具
    pub fn new(workspace_path: &Path) -> Self {
        // 规范化工作空间路径，确保沙箱检查在所有平台上一致
        let canonical_workspace = workspace_path
            .canonicalize()
            .unwrap_or_else(|_| workspace_path.to_path_buf());
        
        Self {
            workspace_path: canonical_workspace,
        }
    }
}

#[async_trait]
impl Tool for FileDeleteTool {
    fn name(&self) -> &str {
        "file_delete"
    }

    fn description(&self) -> &str {
        "删除工作空间中的文件（不可恢复，谨慎使用）"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "要删除的文件相对路径"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String, JiaClawError> {
        let relative_path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JiaClawError::ToolExecution("缺少参数 'path'".to_string()))?;

        let file_path = self.workspace_path.join(relative_path);
        let canonical_path = file_path.canonicalize().unwrap_or(file_path.clone());

        // 安全检查
        if !canonical_path.starts_with(&self.workspace_path) {
            return Err(JiaClawError::ToolExecution(format!(
                "安全错误: 文件 {relative_path} 在工作空间外部"
            )));
        }

        if !canonical_path.exists() {
            return Ok(format!("文件不存在: {relative_path}"));
        }

        if canonical_path.is_dir() {
            return Err(JiaClawError::ToolExecution(format!(
                "路径 {relative_path} 是目录，请使用专门的目录删除工具"
            )));
        }

        // 获取文件大小用于确认消息
        let size = std::fs::metadata(&canonical_path)
            .ok()
            .map_or(0, |m| m.len());

        // 删除文件
        std::fs::remove_file(&canonical_path)
            .map_err(|e| JiaClawError::ToolExecution(format!("无法删除文件: {e}")))?;

        Ok(format!(
            "✅ 文件已删除: {relative_path}\n大小: {size} 字节"
        ))
    }
}

/// File Copy 工具（复制文件）
pub struct FileCopyTool {
    workspace_path: PathBuf,
}

impl FileCopyTool {
    /// 创建新的文件复制工具
    pub fn new(workspace_path: &Path) -> Self {
        // 规范化工作空间路径，确保沙箱检查在所有平台上一致
        let canonical_workspace = workspace_path
            .canonicalize()
            .unwrap_or_else(|_| workspace_path.to_path_buf());
        
        Self {
            workspace_path: canonical_workspace,
        }
    }
}

#[async_trait]
impl Tool for FileCopyTool {
    fn name(&self) -> &str {
        "file_copy"
    }

    fn description(&self) -> &str {
        "在工作空间内复制文件"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "source": {
                    "type": "string",
                    "description": "源文件相对路径"
                },
                "destination": {
                    "type": "string",
                    "description": "目标文件相对路径"
                }
            },
            "required": ["source", "destination"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String, JiaClawError> {
        let source_rel = args
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JiaClawError::ToolExecution("缺少参数 'source'".to_string()))?;

        let dest_rel = args
            .get("destination")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JiaClawError::ToolExecution("缺少参数 'destination'".to_string()))?;

        let source_path = self.workspace_path.join(source_rel);
        let dest_path = self.workspace_path.join(dest_rel);

        // 安全检查源文件
        let source_canonical = source_path
            .canonicalize()
            .map_err(|_| JiaClawError::ToolExecution(format!("源文件不存在: {source_rel}")))?;

        if !source_canonical.starts_with(&self.workspace_path) {
            return Err(JiaClawError::ToolExecution(format!(
                "安全错误: 源文件 {source_rel} 在工作空间外部"
            )));
        }

        // 安全检查目标路径
        let dest_parent = dest_path
            .parent()
            .ok_or_else(|| JiaClawError::ToolExecution("无效的目标路径".to_string()))?;

        // 规范化父目录路径（如果存在）进行安全检查
        let canonical_dest_parent = if dest_parent.exists() {
            dest_parent.canonicalize().unwrap_or_else(|_| dest_parent.to_path_buf())
        } else {
            dest_parent.to_path_buf()
        };
        
        if !canonical_dest_parent.starts_with(&self.workspace_path) {
            return Err(JiaClawError::ToolExecution(format!(
                "安全错误: 目标路径 {dest_rel} 在工作空间外部"
            )));
        }

        if !source_canonical.is_file() {
            return Err(JiaClawError::ToolExecution(format!(
                "源路径 {source_rel} 不是文件"
            )));
        }

        // 创建目标父目录（使用原始路径，因为 canonicalize 需要路径存在）
        std::fs::create_dir_all(dest_parent)
            .map_err(|e| JiaClawError::ToolExecution(format!("无法创建目标目录: {e}")))?;

        // 复制文件
        let bytes_copied = std::fs::copy(&source_canonical, &dest_path)
            .map_err(|e| JiaClawError::ToolExecution(format!("无法复制文件: {e}")))?;

        Ok(format!(
            "✅ 文件已复制:\n  从: {source_rel}\n  到: {dest_rel}\n  大小: {bytes_copied} 字节"
        ))
    }
}

/// Shell Exec 工具（白名单模式）
pub struct ShellExecTool {
    workspace_path: PathBuf,
}

impl ShellExecTool {
    /// 创建新的Shell执行工具
    pub fn new(workspace_path: &Path) -> Self {
        Self {
            workspace_path: workspace_path.to_path_buf(),
        }
    }

    /// 安全命令白名单
    fn is_safe_command(cmd: &str) -> bool {
        const SAFE_COMMANDS: &[&str] = &[
            "ls", "pwd", "echo", "cat", "head", "tail", "wc", "grep", "find", "which", "date",
            "whoami", "hostname", "uname", "env",
        ];

        SAFE_COMMANDS.contains(&cmd)
    }
}

#[async_trait]
impl Tool for ShellExecTool {
    fn name(&self) -> &str {
        "shell_exec"
    }

    fn description(&self) -> &str {
        "执行安全的Shell命令（白名单：ls, pwd, echo, cat, head, tail, wc, grep, find, which, date, whoami, hostname, uname, env）"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "要执行的命令（仅白名单命令）"
                },
                "args": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "命令参数列表（可选）"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String, JiaClawError> {
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| JiaClawError::ToolExecution("缺少参数 'command'".to_string()))?;

        // 白名单检查
        if !Self::is_safe_command(command) {
            return Err(JiaClawError::ToolExecution(format!(
                "命令 '{command}' 不在安全白名单中。\n允许的命令: ls, pwd, echo, cat, head, tail, wc, grep, find, which, date, whoami, hostname, uname, env"
            )));
        }

        let cmd_args = args
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(String::from)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // 在工作空间目录中执行
        let workspace_path = self.workspace_path.clone();
        let command_owned = command.to_string();

        tokio::task::spawn_blocking(move || {
            let output = std::process::Command::new(&command_owned)
                .args(&cmd_args)
                .current_dir(&workspace_path)
                .output()
                .map_err(|e| JiaClawError::ToolExecution(format!("命令执行失败: {e}")))?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let status = output.status;

            let mut result = format!("命令: {} {}\n", command_owned, cmd_args.join(" "));
            result.push_str(&format!("工作目录: {}\n", workspace_path.display()));
            result.push_str(&format!("退出码: {}\n\n", status.code().unwrap_or(-1)));

            if !stdout.is_empty() {
                result.push_str("标准输出:\n");
                result.push_str(&stdout);
                result.push('\n');
            }

            if !stderr.is_empty() {
                result.push_str("标准错误:\n");
                result.push_str(&stderr);
            }

            Ok(result)
        })
        .await
        .map_err(|e| JiaClawError::ToolExecution(format!("任务执行失败: {e}")))?
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

    #[tokio::test]
    async fn test_file_read_tool() {
        let temp_workspace = std::env::temp_dir().join("jiaclaw_test_file_read");
        let _ = fs::remove_dir_all(&temp_workspace);
        fs::create_dir_all(&temp_workspace).unwrap();

        let test_content = "Test file content";
        fs::write(temp_workspace.join("test.txt"), test_content).unwrap();

        let tool = FileReadTool::new(&temp_workspace);

        assert_eq!(tool.name(), "file_read");

        let result = tool
            .execute(serde_json::json!({"path": "test.txt"}))
            .await
            .unwrap();

        assert!(result.contains(test_content));
        assert!(result.contains("test.txt"));

        let _ = fs::remove_dir_all(&temp_workspace);
    }

    #[tokio::test]
    async fn test_file_write_tool() {
        let temp_workspace = std::env::temp_dir().join("jiaclaw_test_file_write");
        let _ = fs::remove_dir_all(&temp_workspace);
        fs::create_dir_all(&temp_workspace).unwrap();

        let tool = FileWriteTool::new(&temp_workspace);

        assert_eq!(tool.name(), "file_write");

        let test_content = "Written content";
        let result = tool
            .execute(serde_json::json!({
                "path": "output.txt",
                "content": test_content
            }))
            .await
            .unwrap();

        assert!(result.contains("已写入"));

        // 验证文件确实被写入
        let file_path = temp_workspace.join("output.txt");
        assert!(file_path.exists());
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, test_content);

        let _ = fs::remove_dir_all(&temp_workspace);
    }

    #[tokio::test]
    async fn test_http_get_tool() {
        let tool = HttpGetTool::new();

        assert_eq!(tool.name(), "http_get");

        // 测试无效URL
        let result = tool
            .execute(serde_json::json!({"url": "invalid-url"}))
            .await;
        assert!(result.is_err());

        // 注意: 实际的HTTP测试需要mock服务器，这里仅测试工具结构
    }

    #[tokio::test]
    async fn test_datetime_tool() {
        let tool = DateTimeTool::new();

        assert_eq!(tool.name(), "datetime_now");

        let result = tool.execute(serde_json::json!({})).await.unwrap();

        assert!(result.contains("Unix时间戳"));
        assert!(result.contains("UTC"));
    }

    #[tokio::test]
    async fn test_json_query_tool() {
        let tool = JsonQueryTool::new();

        assert_eq!(tool.name(), "json_query");

        let test_json = r#"{"user": {"name": "Alice", "age": 30}}"#;

        // 测试完整JSON
        let result = tool
            .execute(serde_json::json!({"json": test_json}))
            .await
            .unwrap();
        assert!(result.contains("Alice"));

        // 测试路径查询
        let result = tool
            .execute(serde_json::json!({
                "json": test_json,
                "path": "user.name"
            }))
            .await
            .unwrap();
        assert!(result.contains("Alice"));

        // 测试无效路径
        let result = tool
            .execute(serde_json::json!({
                "json": test_json,
                "path": "invalid.path"
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_list_tool() {
        let temp_workspace = std::env::temp_dir().join("jiaclaw_test_file_list");
        let _ = fs::remove_dir_all(&temp_workspace);
        fs::create_dir_all(&temp_workspace).unwrap();

        // 创建测试文件和目录
        fs::write(temp_workspace.join("file1.txt"), "content1").unwrap();
        fs::write(temp_workspace.join("file2.txt"), "content2").unwrap();
        fs::create_dir_all(temp_workspace.join("subdir")).unwrap();

        let tool = FileListTool::new(&temp_workspace);

        assert_eq!(tool.name(), "file_list");

        let result = tool.execute(serde_json::json!({})).await.unwrap();

        assert!(result.contains("file1.txt"));
        assert!(result.contains("file2.txt"));
        assert!(result.contains("subdir/"));

        let _ = fs::remove_dir_all(&temp_workspace);
    }

    #[tokio::test]
    async fn test_file_delete_tool() {
        let temp_workspace = std::env::temp_dir().join("jiaclaw_test_file_delete");
        let _ = fs::remove_dir_all(&temp_workspace);
        fs::create_dir_all(&temp_workspace).unwrap();

        let test_file = temp_workspace.join("to_delete.txt");
        fs::write(&test_file, "delete me").unwrap();

        let tool = FileDeleteTool::new(&temp_workspace);

        assert_eq!(tool.name(), "file_delete");

        assert!(test_file.exists());

        let result = tool
            .execute(serde_json::json!({"path": "to_delete.txt"}))
            .await
            .unwrap();

        assert!(result.contains("已删除"));
        assert!(!test_file.exists());

        let _ = fs::remove_dir_all(&temp_workspace);
    }

    #[tokio::test]
    async fn test_file_copy_tool() {
        let temp_workspace = std::env::temp_dir().join("jiaclaw_test_file_copy");
        let _ = fs::remove_dir_all(&temp_workspace);
        fs::create_dir_all(&temp_workspace).unwrap();

        let test_content = "copy this content";
        fs::write(temp_workspace.join("source.txt"), test_content).unwrap();

        let tool = FileCopyTool::new(&temp_workspace);

        assert_eq!(tool.name(), "file_copy");

        let result = tool
            .execute(serde_json::json!({
                "source": "source.txt",
                "destination": "dest.txt"
            }))
            .await
            .unwrap();

        assert!(result.contains("已复制"));

        let dest_path = temp_workspace.join("dest.txt");
        assert!(dest_path.exists());
        let content = fs::read_to_string(&dest_path).unwrap();
        assert_eq!(content, test_content);

        let _ = fs::remove_dir_all(&temp_workspace);
    }

    #[tokio::test]
    async fn test_shell_exec_tool() {
        let temp_workspace = std::env::temp_dir().join("jiaclaw_test_shell");
        let _ = fs::remove_dir_all(&temp_workspace);
        fs::create_dir_all(&temp_workspace).unwrap();

        let tool = ShellExecTool::new(&temp_workspace);

        assert_eq!(tool.name(), "shell_exec");

        // 测试安全命令
        let result = tool
            .execute(serde_json::json!({
                "command": "echo",
                "args": ["hello", "world"]
            }))
            .await
            .unwrap();

        assert!(result.contains("hello world") || result.contains("命令: echo"));

        // 测试不安全命令
        let result = tool
            .execute(serde_json::json!({
                "command": "rm",
                "args": ["-rf", "/"]
            }))
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("不在安全白名单中"));

        let _ = fs::remove_dir_all(&temp_workspace);
    }
}
