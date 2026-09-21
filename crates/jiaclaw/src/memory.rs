// Copyright 2026 JiaClaw contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! 工作区 `MEMORY.md` 长期记忆：路径校验、提示注入读取、原子写入。

#![allow(clippy::module_name_repetitions)]

use crate::tools::Tool;
use async_trait::async_trait;
use jiaclaw_core::{JiaClawError, MEMORY_PROMPT_MAX_BYTES};
use serde_json::Value;
use std::path::{Component, Path, PathBuf};

/// 记忆文件在磁盘上的状态（供 `doctor` / CLI 使用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryFileStatus {
    /// 解析后的绝对或工作区拼接路径
    pub path: PathBuf,
    /// 文件是否存在
    pub exists: bool,
    /// 文件大小（字节）；不存在时为 0
    pub size_bytes: u64,
}

/// 将配置中的相对路径解析为工作区内的记忆文件路径。
///
/// 拒绝绝对路径和任何 `..` 组件，防止路径穿越。
///
/// # Errors
///
/// 路径为空、绝对路径、包含 `..`，或不落在工作空间内时返回错误。
pub fn resolve_memory_path(workspace: &Path, configured: &str) -> Result<PathBuf, JiaClawError> {
    let configured = configured.trim();
    if configured.is_empty() {
        return Err(JiaClawError::Configuration("记忆路径不能为空".to_string()));
    }

    let rel = Path::new(configured);
    if rel.is_absolute() {
        return Err(JiaClawError::Configuration(format!(
            "记忆路径必须相对于工作空间，禁止绝对路径: {configured}"
        )));
    }

    for component in rel.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => {
                return Err(JiaClawError::Configuration(format!(
                    "记忆路径禁止路径穿越 (..): {configured}"
                )));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(JiaClawError::Configuration(format!(
                    "记忆路径必须相对于工作空间: {configured}"
                )));
            }
        }
    }

    let workspace_base = canonicalize_existing_or_clone(workspace);
    let joined = workspace_base.join(rel);

    ensure_path_within_workspace(&workspace_base, &joined)?;
    Ok(joined)
}

/// 读取记忆文件以注入系统提示。
///
/// 文件不存在或（trim 后）为空时返回 `Ok(None)`，不报错。
/// 超过 [`MEMORY_PROMPT_MAX_BYTES`] 时截断到 UTF-8 边界并 `warn`。
///
/// # Errors
///
/// 路径非法，或文件存在但无法读取时返回错误。
pub fn load_memory_for_prompt(
    workspace: &Path,
    configured: &str,
) -> Result<Option<String>, JiaClawError> {
    let path = resolve_memory_path(workspace, configured)?;
    if !path.exists() {
        return Ok(None);
    }
    if !path.is_file() {
        tracing::warn!(path = %path.display(), "MEMORY 路径存在但不是文件，跳过注入");
        return Ok(None);
    }

    ensure_existing_within_workspace(workspace, &path)?;

    let raw = std::fs::read_to_string(&path).map_err(|e| {
        JiaClawError::Configuration(format!("无法读取记忆文件 {}: {e}", path.display()))
    })?;

    if raw.trim().is_empty() {
        return Ok(None);
    }

    if raw.len() > MEMORY_PROMPT_MAX_BYTES {
        tracing::warn!(
            path = %path.display(),
            size_bytes = raw.len(),
            limit_bytes = MEMORY_PROMPT_MAX_BYTES,
            "MEMORY 文件过大，截断后注入系统提示"
        );
        Ok(Some(
            truncate_utf8(&raw, MEMORY_PROMPT_MAX_BYTES).to_string(),
        ))
    } else {
        Ok(Some(raw))
    }
}

/// 检查记忆文件是否存在及其大小。
///
/// # Errors
///
/// 配置路径非法时返回错误。
pub fn inspect_memory_file(
    workspace: &Path,
    configured: &str,
) -> Result<MemoryFileStatus, JiaClawError> {
    let path = resolve_memory_path(workspace, configured)?;
    if path.is_file() {
        let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        Ok(MemoryFileStatus {
            path,
            exists: true,
            size_bytes,
        })
    } else {
        Ok(MemoryFileStatus {
            path,
            exists: false,
            size_bytes: 0,
        })
    }
}

/// 写入记忆文件：默认追加一段 Markdown；`replace = true` 时覆盖。
///
/// 只能写约定路径。追加时在已有内容与新段落之间插入换行分隔，并以临时文件 + rename 原子落盘。
///
/// # Errors
///
/// 路径非法、越出工作空间，或 IO 失败时返回错误。
pub fn write_memory(
    workspace: &Path,
    configured: &str,
    content: &str,
    replace: bool,
) -> Result<PathBuf, JiaClawError> {
    let path = resolve_memory_path(workspace, configured)?;
    ensure_path_within_workspace(workspace, &path)?;

    if path.exists() {
        ensure_existing_within_workspace(workspace, &path)?;
        if !path.is_file() {
            return Err(JiaClawError::ToolExecution(format!(
                "记忆路径不是文件: {}",
                path.display()
            )));
        }
    }

    let new_contents = if replace || !path.exists() {
        content.to_string()
    } else {
        let existing = std::fs::read_to_string(&path).map_err(|e| {
            JiaClawError::ToolExecution(format!("无法读取记忆文件 {}: {e}", path.display()))
        })?;
        join_memory_append(&existing, content)
    };

    atomic_write(&path, &new_contents)?;
    Ok(path)
}

/// 追加时用空行分隔已有内容与新段落。
#[must_use]
pub fn join_memory_append(existing: &str, addition: &str) -> String {
    if existing.is_empty() {
        return addition.to_string();
    }
    let mut out = existing.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(addition);
    out
}

/// 按 UTF-8 字符边界截断到最多 `max_bytes` 字节。
#[must_use]
pub fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn canonicalize_existing_or_clone(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn ensure_path_within_workspace(workspace: &Path, target: &Path) -> Result<(), JiaClawError> {
    let ws = canonicalize_existing_or_clone(workspace);

    if target.exists() {
        let check = canonicalize_existing_or_clone(target);
        if check.starts_with(&ws) {
            return Ok(());
        }
        return Err(JiaClawError::ToolExecution(format!(
            "安全错误: 记忆文件 {} 不在工作空间 {} 内",
            target.display(),
            ws.display()
        )));
    }

    if target.starts_with(&ws) {
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        let parent_check = if parent.exists() {
            canonicalize_existing_or_clone(parent)
        } else {
            parent.to_path_buf()
        };
        if parent_check.starts_with(&ws) {
            return Ok(());
        }
    }

    Err(JiaClawError::ToolExecution(format!(
        "安全错误: 记忆文件 {} 不在工作空间 {} 内",
        target.display(),
        ws.display()
    )))
}

fn ensure_existing_within_workspace(workspace: &Path, path: &Path) -> Result<(), JiaClawError> {
    let ws = canonicalize_existing_or_clone(workspace);
    let canon = path.canonicalize().map_err(|e| {
        JiaClawError::ToolExecution(format!("无法解析记忆文件 {}: {e}", path.display()))
    })?;
    if !canon.starts_with(&ws) {
        return Err(JiaClawError::ToolExecution(format!(
            "安全错误: 记忆文件 {} 指向工作空间外部",
            path.display()
        )));
    }
    Ok(())
}

fn atomic_write(path: &Path, contents: &str) -> Result<(), JiaClawError> {
    let parent = path.parent().ok_or_else(|| {
        JiaClawError::ToolExecution(format!("无效的记忆文件路径: {}", path.display()))
    })?;
    std::fs::create_dir_all(parent).map_err(|e| {
        JiaClawError::ToolExecution(format!("无法创建记忆文件目录 {}: {e}", parent.display()))
    })?;

    let file_name = path.file_name().ok_or_else(|| {
        JiaClawError::ToolExecution(format!("无效的记忆文件名: {}", path.display()))
    })?;
    let tmp = parent.join(format!("{}.tmp", file_name.to_string_lossy()));

    std::fs::write(&tmp, contents).map_err(|e| {
        JiaClawError::ToolExecution(format!("无法写入临时记忆文件 {}: {e}", tmp.display()))
    })?;

    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        JiaClawError::ToolExecution(format!(
            "无法提交记忆文件 {} -> {}: {e}",
            tmp.display(),
            path.display()
        ))
    })?;

    Ok(())
}

/// `memory_append` 工具：向约定 MEMORY 路径追加或覆盖 Markdown。
pub struct MemoryAppendTool {
    workspace_path: PathBuf,
    memory_rel_path: String,
}

impl MemoryAppendTool {
    /// 创建工具；`memory_rel_path` 相对工作空间，默认 `MEMORY.md`。
    #[must_use]
    pub fn new(workspace_path: &Path, memory_rel_path: impl Into<String>) -> Self {
        let canonical_workspace = canonicalize_existing_or_clone(workspace_path);
        Self {
            workspace_path: canonical_workspace,
            memory_rel_path: memory_rel_path.into(),
        }
    }
}

#[async_trait]
impl Tool for MemoryAppendTool {
    fn name(&self) -> &str {
        "memory_append"
    }

    fn description(&self) -> &str {
        "将一段 Markdown 写入工作区长期记忆文件（默认 MEMORY.md）。replace=false 时追加并换行分隔；replace=true 时覆盖整个文件。只能写约定路径。"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "要写入的 Markdown 内容"
                },
                "replace": {
                    "type": "boolean",
                    "description": "true 覆盖整个记忆文件；false（默认）追加",
                    "default": false
                }
            },
            "required": ["content"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String, JiaClawError> {
        let content = args
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| JiaClawError::ToolExecution("缺少参数 'content'".to_string()))?;

        let replace = args
            .get("replace")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let path = write_memory(
            &self.workspace_path,
            &self.memory_rel_path,
            content,
            replace,
        )?;

        let metadata = std::fs::metadata(&path).ok();
        let size = metadata.map_or(0, |m| m.len());
        let mode = if replace { "覆盖" } else { "追加" };

        Ok(format!(
            "✅ 长期记忆已{mode}: {}\n大小: {size} 字节\n约定路径: {}",
            path.display(),
            self.memory_rel_path
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiaclaw_core::DEFAULT_MEMORY_PATH;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}_{}_{nanos}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolve_default_path() {
        let ws = unique_temp("jiaclaw_mem_resolve");
        let path = resolve_memory_path(&ws, DEFAULT_MEMORY_PATH).unwrap();
        assert_eq!(path, ws.join("MEMORY.md"));
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn custom_relative_path_within_workspace() {
        let ws = unique_temp("jiaclaw_mem_custom");
        fs::create_dir_all(ws.join("notes")).unwrap();
        fs::write(ws.join("notes/MEMORY.md"), "custom-path-fact").unwrap();
        let loaded = load_memory_for_prompt(&ws, "notes/MEMORY.md")
            .unwrap()
            .expect("content");
        assert!(loaded.contains("custom-path-fact"));
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn resolve_rejects_parent_dir() {
        let ws = unique_temp("jiaclaw_mem_parent");
        let err = resolve_memory_path(&ws, "../secret.md").unwrap_err();
        assert!(err.to_string().contains("穿越"));
        let err = resolve_memory_path(&ws, "foo/../../etc/passwd").unwrap_err();
        assert!(err.to_string().contains("穿越"));
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn resolve_rejects_absolute_path() {
        let ws = unique_temp("jiaclaw_mem_abs");
        let err = resolve_memory_path(&ws, "/etc/passwd").unwrap_err();
        assert!(err.to_string().contains("绝对路径"));
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn load_missing_file_is_none() {
        let ws = unique_temp("jiaclaw_mem_missing");
        let loaded = load_memory_for_prompt(&ws, DEFAULT_MEMORY_PATH).unwrap();
        assert!(loaded.is_none());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn load_empty_file_is_none() {
        let ws = unique_temp("jiaclaw_mem_empty");
        fs::write(ws.join("MEMORY.md"), "   \n\t\n").unwrap();
        let loaded = load_memory_for_prompt(&ws, DEFAULT_MEMORY_PATH).unwrap();
        assert!(loaded.is_none());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn load_returns_content_verbatim() {
        let ws = unique_temp("jiaclaw_mem_load");
        let body = "# Facts\n\n- prefers rust\n";
        fs::write(ws.join("MEMORY.md"), body).unwrap();
        let loaded = load_memory_for_prompt(&ws, DEFAULT_MEMORY_PATH)
            .unwrap()
            .expect("content");
        assert_eq!(loaded, body);
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn load_truncates_oversize() {
        let ws = unique_temp("jiaclaw_mem_trunc");
        let mut body = "héllo".repeat(MEMORY_PROMPT_MAX_BYTES);
        body.push('中');
        fs::write(ws.join("MEMORY.md"), &body).unwrap();
        let loaded = load_memory_for_prompt(&ws, DEFAULT_MEMORY_PATH)
            .unwrap()
            .expect("truncated");
        assert!(loaded.len() <= MEMORY_PROMPT_MAX_BYTES);
        assert!(loaded.is_char_boundary(loaded.len()));
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn append_creates_and_separates() {
        let ws = unique_temp("jiaclaw_mem_append");
        write_memory(&ws, DEFAULT_MEMORY_PATH, "first", false).unwrap();
        write_memory(&ws, DEFAULT_MEMORY_PATH, "second", false).unwrap();
        let text = fs::read_to_string(ws.join("MEMORY.md")).unwrap();
        assert_eq!(text, "first\n\nsecond");
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn replace_overwrites() {
        let ws = unique_temp("jiaclaw_mem_replace");
        write_memory(&ws, DEFAULT_MEMORY_PATH, "old", false).unwrap();
        write_memory(&ws, DEFAULT_MEMORY_PATH, "new", true).unwrap();
        let text = fs::read_to_string(ws.join("MEMORY.md")).unwrap();
        assert_eq!(text, "new");
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn write_rejects_path_traversal() {
        let ws = unique_temp("jiaclaw_mem_write_trav");
        let err = write_memory(&ws, "../evil.md", "nope", false).unwrap_err();
        assert!(err.to_string().contains("穿越") || err.to_string().contains("安全"));
        assert!(!ws.parent().unwrap().join("evil.md").exists());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn inspect_reports_size() {
        let ws = unique_temp("jiaclaw_mem_inspect");
        let missing = inspect_memory_file(&ws, DEFAULT_MEMORY_PATH).unwrap();
        assert!(!missing.exists);
        assert_eq!(missing.size_bytes, 0);

        fs::write(ws.join("MEMORY.md"), "abcd").unwrap();
        let present = inspect_memory_file(&ws, DEFAULT_MEMORY_PATH).unwrap();
        assert!(present.exists);
        assert_eq!(present.size_bytes, 4);
        let _ = fs::remove_dir_all(&ws);
    }

    #[tokio::test]
    async fn tool_append_is_visible_on_disk() {
        let ws = unique_temp("jiaclaw_mem_tool");
        let tool = MemoryAppendTool::new(&ws, DEFAULT_MEMORY_PATH);
        assert_eq!(tool.name(), "memory_append");

        let result = tool
            .execute(serde_json::json!({"content": "- likes tea"}))
            .await
            .unwrap();
        assert!(result.contains("追加"));
        let text = fs::read_to_string(ws.join("MEMORY.md")).unwrap();
        assert!(text.contains("- likes tea"));
        let _ = fs::remove_dir_all(&ws);
    }

    #[tokio::test]
    async fn tool_ignores_path_argument() {
        let ws = unique_temp("jiaclaw_mem_ignore_path");
        let tool = MemoryAppendTool::new(&ws, DEFAULT_MEMORY_PATH);
        tool.execute(serde_json::json!({
            "content": "safe",
            "path": "../evil.md"
        }))
        .await
        .unwrap();
        assert!(ws.join("MEMORY.md").exists());
        assert!(!ws.join("evil.md").exists());
        let _ = fs::remove_dir_all(&ws);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_rejected_on_read() {
        let ws = unique_temp("jiaclaw_mem_symlink");
        let outside = ws.parent().unwrap().join(format!(
            "jiaclaw_mem_outside_{}",
            ws.file_name().unwrap().to_string_lossy()
        ));
        fs::write(&outside, "secret-outside").unwrap();
        std::os::unix::fs::symlink(&outside, ws.join("MEMORY.md")).unwrap();

        let result = load_memory_for_prompt(&ws, DEFAULT_MEMORY_PATH);
        assert!(result.is_err(), "symlink 逃逸应被拒绝: {result:?}");

        let _ = fs::remove_file(&outside);
        let _ = fs::remove_dir_all(&ws);
    }
}
