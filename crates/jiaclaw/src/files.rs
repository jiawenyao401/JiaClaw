// Copyright 2026 JiaClaw contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! 工作区只读文件工具：`read_file` / `list_dir`。
//!
//! 路径解析复用 [`crate::memory::resolve_workspace_relative_path`]（禁 `..`、绝对路径、symlink 逃逸）。
//! 不调用 LLM，不执行 shell。

use crate::memory::{
    canonicalize_existing_or_clone, ensure_existing_within_workspace,
    resolve_workspace_relative_path,
};
use crate::tools::Tool;
use async_trait::async_trait;
use jiaclaw_core::JiaClawError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// `read_file` 单文件读取上限（字节）。超过则明确报错，不截断返回。
pub const READ_FILE_MAX_BYTES: usize = 256 * 1024;

/// `list_dir` 的 `max_entries` 缺省值
pub const LIST_DIR_DEFAULT_MAX_ENTRIES: usize = 200;

/// `list_dir` 的 `max_entries` 上限（含）
pub const LIST_DIR_MAX_ENTRIES: usize = 1000;

/// 解析后的 `read_file` 参数。`offset` 为 **1-indexed 行号**；`limit` 为行数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadFileArgs {
    /// 工作区相对路径
    pub path: String,
    /// 起始行（从 1 起；缺省为 1）
    pub offset: usize,
    /// 最多返回的行数；`None` 表示读到文件末尾（仍受大小上限约束）
    pub limit: Option<usize>,
}

/// 解析后的 `list_dir` 参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListDirArgs {
    /// 工作区相对路径（缺省 `.`）
    pub path: String,
    /// 最多返回的条目数（钳制 1..=1000）
    pub max_entries: usize,
    /// 是否递归（默认 `false`）
    pub recursive: bool,
}

/// 将 `max_entries` 钳制到 `1..=1000`。
#[must_use]
pub fn clamp_list_dir_max_entries(raw: u64) -> usize {
    usize::try_from(raw)
        .unwrap_or(LIST_DIR_MAX_ENTRIES)
        .clamp(1, LIST_DIR_MAX_ENTRIES)
}

fn parse_positive_usize(value: &Value, name: &str) -> Result<usize, JiaClawError> {
    let raw = value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()));
    match raw {
        Some(n) => usize::try_from(n)
            .map_err(|_| JiaClawError::ToolExecution(format!("参数 '{name}' 超出范围"))),
        None => Err(JiaClawError::ToolExecution(format!(
            "参数 '{name}' 必须是正整数"
        ))),
    }
}

/// 解析 `read_file` 参数：必填 `path`；可选 `offset` / `limit`（按行，1-indexed）。
///
/// # Errors
///
/// `path` 缺失/空白，或 `offset`/`limit` 不是整数时返回错误。
pub fn parse_read_file_args(args: &Value) -> Result<ReadFileArgs, JiaClawError> {
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            JiaClawError::ToolExecution("缺少参数 'path'（工作区相对路径）".to_string())
        })?;

    let offset = match args.get("offset") {
        None | Some(Value::Null) => 1,
        Some(value) => {
            let n = parse_positive_usize(value, "offset")?;
            n.max(1)
        }
    };

    let limit = match args.get("limit") {
        None | Some(Value::Null) => None,
        Some(value) => Some(parse_positive_usize(value, "limit")?.max(1)),
    };

    Ok(ReadFileArgs {
        path: path.to_string(),
        offset,
        limit,
    })
}

/// 解析 `list_dir` 参数：可选 `path`（默认 `.`）、`max_entries`（默认 200）、`recursive`（默认 false）。
///
/// # Errors
///
/// `max_entries` 不是整数，或 `recursive` 不是布尔值时返回错误。
pub fn parse_list_dir_args(args: &Value) -> Result<ListDirArgs, JiaClawError> {
    let path = match args.get("path") {
        None | Some(Value::Null) => ".".to_string(),
        Some(value) => {
            let raw = value.as_str().ok_or_else(|| {
                JiaClawError::ToolExecution(
                    "参数 'path' 必须是字符串（工作区相对路径，默认 .）".to_string(),
                )
            })?;
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                ".".to_string()
            } else {
                trimmed.to_string()
            }
        }
    };

    let max_entries = match args.get("max_entries") {
        None | Some(Value::Null) => LIST_DIR_DEFAULT_MAX_ENTRIES,
        Some(value) => {
            let raw = value
                .as_u64()
                .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()));
            match raw {
                Some(n) => clamp_list_dir_max_entries(n),
                None => {
                    return Err(JiaClawError::ToolExecution(
                        "参数 'max_entries' 必须是整数（将钳制到 1..=1000）".to_string(),
                    ));
                }
            }
        }
    };

    let recursive = match args.get("recursive") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(_) => {
            return Err(JiaClawError::ToolExecution(
                "参数 'recursive' 必须是布尔值（默认 false）".to_string(),
            ));
        }
    };

    Ok(ListDirArgs {
        path,
        max_entries,
        recursive,
    })
}

fn slice_lines(content: &str, offset: usize, limit: Option<usize>) -> (String, usize, usize, bool) {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    let start = offset.saturating_sub(1).min(total_lines);
    let end = match limit {
        Some(n) => start.saturating_add(n).min(total_lines),
        None => total_lines,
    };
    let returned = &lines[start..end];
    let truncated = start > 0 || end < total_lines;
    (returned.join("\n"), total_lines, returned.len(), truncated)
}

/// 读取工作区相对路径下的真实文本文件（按行切片）。
///
/// # Errors
///
/// 路径非法、越出工作空间、symlink 逃逸、不是文件、超过大小上限、或二进制时返回错误。
pub fn read_workspace_file(
    workspace: &Path,
    rel_path: &str,
    offset: usize,
    limit: Option<usize>,
    max_bytes: usize,
) -> Result<ReadFileOutput, JiaClawError> {
    let path = resolve_workspace_relative_path(workspace, rel_path)?;
    if !path.exists() {
        return Err(JiaClawError::ToolExecution(format!(
            "文件不存在: {rel_path}"
        )));
    }
    ensure_existing_within_workspace(workspace, &path)?;

    let canon = path
        .canonicalize()
        .map_err(|e| JiaClawError::ToolExecution(format!("无法解析文件 {rel_path}: {e}")))?;
    if !canon.is_file() {
        return Err(JiaClawError::ToolExecution(format!(
            "路径不是文件: {rel_path}"
        )));
    }

    let size_bytes = fs::metadata(&canon)
        .map(|m| m.len())
        .map_err(|e| JiaClawError::ToolExecution(format!("无法读取文件元数据 {rel_path}: {e}")))?;
    let limit_bytes = u64::try_from(max_bytes).unwrap_or(u64::MAX);
    if size_bytes > limit_bytes {
        return Err(JiaClawError::ToolExecution(format!(
            "文件超过上限 {max_bytes} 字节（实际 {size_bytes} 字节）: {rel_path}"
        )));
    }

    let bytes = fs::read(&canon)
        .map_err(|e| JiaClawError::ToolExecution(format!("无法读取文件 {rel_path}: {e}")))?;
    let content = match String::from_utf8(bytes) {
        Ok(text) if !text.contains('\0') => text,
        _ => {
            return Err(JiaClawError::ToolExecution(format!(
                "二进制文件，拒绝读取: {rel_path}（检测到 NUL 或非 UTF-8）"
            )));
        }
    };

    let (text, total_lines, returned_lines, truncated) = slice_lines(&content, offset, limit);
    Ok(ReadFileOutput {
        path: rel_path.to_string(),
        offset,
        limit,
        total_lines,
        returned_lines,
        truncated,
        size_bytes,
        content: text,
    })
}

/// 列出工作区相对目录（默认不递归，不跟随 symlink 目录）。
///
/// # Errors
///
/// 路径非法、越出工作空间、symlink 逃逸、或不是目录时返回错误。
pub fn list_workspace_dir(
    workspace: &Path,
    rel_path: &str,
    max_entries: usize,
    recursive: bool,
) -> Result<ListDirOutput, JiaClawError> {
    let path = resolve_workspace_relative_path(workspace, rel_path)?;
    if !path.exists() {
        return Err(JiaClawError::ToolExecution(format!(
            "目录不存在: {rel_path}"
        )));
    }
    ensure_existing_within_workspace(workspace, &path)?;

    let canon = path
        .canonicalize()
        .map_err(|e| JiaClawError::ToolExecution(format!("无法解析目录 {rel_path}: {e}")))?;
    if !canon.is_dir() {
        return Err(JiaClawError::ToolExecution(format!(
            "路径不是目录: {rel_path}"
        )));
    }

    let mut entries = Vec::new();
    let truncated = collect_dir_entries(&canon, "", recursive, max_entries, &mut entries)?;
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    if entries.len() > max_entries {
        entries.truncate(max_entries);
    }

    Ok(ListDirOutput {
        path: rel_path.to_string(),
        recursive,
        truncated,
        entries,
    })
}

fn collect_dir_entries(
    dir: &Path,
    prefix: &str,
    recursive: bool,
    max_entries: usize,
    out: &mut Vec<DirEntryInfo>,
) -> Result<bool, JiaClawError> {
    let mut truncated = false;
    let mut children: Vec<(String, PathBuf, fs::Metadata)> = Vec::new();

    let iter = fs::read_dir(dir)
        .map_err(|e| JiaClawError::ToolExecution(format!("无法读取目录 {}: {e}", dir.display())))?;
    for entry in iter {
        let entry =
            entry.map_err(|e| JiaClawError::ToolExecution(format!("无法读取目录条目: {e}")))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "." || name == ".." {
            continue;
        }
        let child_path = entry.path();
        let meta = match entry.symlink_metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        children.push((name.into_owned(), child_path, meta));
    }
    children.sort_by(|a, b| a.0.cmp(&b.0));

    for (name, child_path, meta) in children {
        if out.len() >= max_entries {
            truncated = true;
            break;
        }
        let rel_name = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let is_dir = meta.file_type().is_dir();
        let kind = if is_dir { "dir" } else { "file" };
        let size = if is_dir { None } else { Some(meta.len()) };
        out.push(DirEntryInfo {
            name: rel_name.clone(),
            kind: kind.to_string(),
            size,
        });

        if recursive && is_dir && out.len() < max_entries {
            let nested_truncated =
                collect_dir_entries(&child_path, &rel_name, true, max_entries, out)?;
            if nested_truncated {
                truncated = true;
                break;
            }
        }
    }

    Ok(truncated)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadFileOutput {
    /// 调用方传入的工作区相对路径
    pub path: String,
    /// 实际使用的起始行（从 1 起）
    pub offset: usize,
    /// 请求的最大行数（缺省为读到末尾）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// 文件总行数
    pub total_lines: usize,
    /// 本次返回的行数
    pub returned_lines: usize,
    /// 是否因 offset/limit 未返回全文
    pub truncated: bool,
    /// 文件字节数
    pub size_bytes: u64,
    /// 切片后的文本内容
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntryInfo {
    /// 条目名称（递归时为相对所列目录的路径）
    pub name: String,
    /// `file` 或 `dir`（不跟随 symlink 判断类型）
    #[serde(rename = "type")]
    pub kind: String,
    /// 文件大小（字节）；目录为 `None`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListDirOutput {
    /// 所列目录的工作区相对路径
    pub path: String,
    /// 是否递归
    pub recursive: bool,
    /// 是否因 `max_entries` 截断
    pub truncated: bool,
    /// 目录条目
    pub entries: Vec<DirEntryInfo>,
}

/// `read_file` 工具：读取工作区相对路径下的文本文件（路径沙箱，不调用 LLM）。
pub struct WorkspaceReadFileTool {
    workspace_path: PathBuf,
    max_bytes: usize,
}

impl WorkspaceReadFileTool {
    /// 创建工具；读取上限为 [`READ_FILE_MAX_BYTES`]。
    #[must_use]
    pub fn new(workspace_path: &Path) -> Self {
        Self {
            workspace_path: canonicalize_existing_or_clone(workspace_path),
            max_bytes: READ_FILE_MAX_BYTES,
        }
    }

    /// 测试或自定义上限。
    #[must_use]
    pub fn with_max_bytes(workspace_path: &Path, max_bytes: usize) -> Self {
        Self {
            workspace_path: canonicalize_existing_or_clone(workspace_path),
            max_bytes,
        }
    }
}

#[async_trait]
impl Tool for WorkspaceReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "读取工作区相对路径下的文本文件。path 相对于工作区根；禁止 .. / 绝对路径 / symlink 逃逸。可选 offset/limit 按行切片（1-indexed）。超过 256KiB 或二进制则报错。返回 {path, offset, limit, total_lines, returned_lines, truncated, size_bytes, content}。"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "文件相对路径（相对于工作区根目录）"
                },
                "offset": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "起始行号（从 1 起，默认 1）",
                    "default": 1
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "最多返回的行数；缺省读到文件末尾（仍受 256KiB 上限约束）"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String, JiaClawError> {
        let parsed = parse_read_file_args(&args)?;
        let output = read_workspace_file(
            &self.workspace_path,
            &parsed.path,
            parsed.offset,
            parsed.limit,
            self.max_bytes,
        )?;
        serde_json::to_string_pretty(&output)
            .map_err(|e| JiaClawError::ToolExecution(format!("序列化读取结果失败: {e}")))
    }
}

/// `list_dir` 工具：列出工作区相对目录（默认不递归，不调用 LLM）。
pub struct WorkspaceListDirTool {
    workspace_path: PathBuf,
}

impl WorkspaceListDirTool {
    /// 创建工具。
    #[must_use]
    pub fn new(workspace_path: &Path) -> Self {
        Self {
            workspace_path: canonicalize_existing_or_clone(workspace_path),
        }
    }
}

#[async_trait]
impl Tool for WorkspaceListDirTool {
    fn name(&self) -> &str {
        "list_dir"
    }

    fn description(&self) -> &str {
        "列出工作区相对目录中的条目。path 默认 . ；禁止 .. / 绝对路径 / symlink 逃逸。默认不递归（recursive=false）。可选 max_entries（默认 200，钳制 1..=1000）。返回 {path, recursive, truncated, entries:[{name, type, size?}]}。不执行 shell。"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "目录相对路径（相对于工作区根目录，默认 .）",
                    "default": "."
                },
                "max_entries": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "最多返回的条目数（默认 200，钳制 1..=1000）",
                    "default": 200
                },
                "recursive": {
                    "type": "boolean",
                    "description": "是否递归列出子目录（默认 false，且不跟随 symlink 目录）",
                    "default": false
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: Value) -> Result<String, JiaClawError> {
        let parsed = parse_list_dir_args(&args)?;
        let output = list_workspace_dir(
            &self.workspace_path,
            &parsed.path,
            parsed.max_entries,
            parsed.recursive,
        )?;
        serde_json::to_string_pretty(&output)
            .map_err(|e| JiaClawError::ToolExecution(format!("序列化目录列表失败: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn parse_read_file_args_requires_path_and_defaults_offset() {
        let err = parse_read_file_args(&serde_json::json!({}))
            .unwrap_err()
            .to_string();
        assert!(err.contains("path"), "{err}");

        let err = parse_read_file_args(&serde_json::json!({"path": "   "}))
            .unwrap_err()
            .to_string();
        assert!(err.contains("path"), "{err}");

        let parsed = parse_read_file_args(&serde_json::json!({"path": " notes/a.md "})).unwrap();
        assert_eq!(parsed.path, "notes/a.md");
        assert_eq!(parsed.offset, 1);
        assert!(parsed.limit.is_none());

        let parsed = parse_read_file_args(&serde_json::json!({
            "path": "a.md",
            "offset": 3,
            "limit": 2
        }))
        .unwrap();
        assert_eq!(parsed.offset, 3);
        assert_eq!(parsed.limit, Some(2));
    }

    #[test]
    fn parse_list_dir_args_defaults_and_clamps() {
        let parsed = parse_list_dir_args(&serde_json::json!({})).unwrap();
        assert_eq!(parsed.path, ".");
        assert_eq!(parsed.max_entries, LIST_DIR_DEFAULT_MAX_ENTRIES);
        assert!(!parsed.recursive);

        let parsed = parse_list_dir_args(&serde_json::json!({
            "path": " notes ",
            "max_entries": 0,
            "recursive": true
        }))
        .unwrap();
        assert_eq!(parsed.path, "notes");
        assert_eq!(parsed.max_entries, 1);
        assert!(parsed.recursive);

        let parsed = parse_list_dir_args(&serde_json::json!({"max_entries": 9999})).unwrap();
        assert_eq!(parsed.max_entries, LIST_DIR_MAX_ENTRIES);
    }

    #[test]
    fn clamp_list_dir_max_entries_bounds() {
        assert_eq!(clamp_list_dir_max_entries(0), 1);
        assert_eq!(clamp_list_dir_max_entries(200), 200);
        assert_eq!(clamp_list_dir_max_entries(1000), 1000);
        assert_eq!(clamp_list_dir_max_entries(1001), 1000);
    }

    #[tokio::test]
    async fn read_file_success_and_line_slice() {
        let ws = unique_temp("jiaclaw_read_file_ok");
        fs::write(ws.join("notes.md"), "alpha\nbeta\ngamma\n").unwrap();
        let tool = WorkspaceReadFileTool::new(&ws);
        assert_eq!(tool.name(), "read_file");

        let result = tool
            .execute(serde_json::json!({"path": "notes.md"}))
            .await
            .unwrap();
        assert!(result.contains("alpha"), "{result}");
        assert!(result.contains("gamma"), "{result}");
        assert!(result.contains("\"total_lines\": 3"), "{result}");

        let sliced = tool
            .execute(serde_json::json!({
                "path": "notes.md",
                "offset": 2,
                "limit": 1
            }))
            .await
            .unwrap();
        assert!(sliced.contains("beta"), "{sliced}");
        assert!(!sliced.contains("alpha"), "{sliced}");
        assert!(sliced.contains("\"returned_lines\": 1"), "{sliced}");
        let _ = fs::remove_dir_all(&ws);
    }

    #[tokio::test]
    async fn read_file_rejects_traversal_and_absolute() {
        let ws = unique_temp("jiaclaw_read_file_trav");
        fs::write(ws.join("ok.md"), "inside").unwrap();
        let tool = WorkspaceReadFileTool::new(&ws);

        let err = tool
            .execute(serde_json::json!({"path": "../secret.md"}))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("穿越") || err.contains("安全"), "{err}");

        let err = tool
            .execute(serde_json::json!({"path": "/etc/passwd"}))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("绝对路径"), "{err}");
        let _ = fs::remove_dir_all(&ws);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn read_file_rejects_symlink_escape() {
        let ws = unique_temp("jiaclaw_read_file_symlink");
        let outside = ws.parent().unwrap().join(format!(
            "jiaclaw_read_outside_{}",
            ws.file_name().unwrap().to_string_lossy()
        ));
        fs::write(&outside, "secret-outside").unwrap();
        std::os::unix::fs::symlink(&outside, ws.join("leak.md")).unwrap();

        let tool = WorkspaceReadFileTool::new(&ws);
        let result = tool.execute(serde_json::json!({"path": "leak.md"})).await;
        assert!(result.is_err(), "symlink 逃逸应被拒绝: {result:?}");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("安全") || err.contains("工作空间"), "{err}");

        let _ = fs::remove_file(&outside);
        let _ = fs::remove_dir_all(&ws);
    }

    #[tokio::test]
    async fn read_file_rejects_oversize_and_binary() {
        let ws = unique_temp("jiaclaw_read_file_limits");
        let tool = WorkspaceReadFileTool::with_max_bytes(&ws, 16);
        fs::write(ws.join("big.md"), "abcdefghijklmnopqrstuvwxyz").unwrap();
        let err = tool
            .execute(serde_json::json!({"path": "big.md"}))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("超过上限"), "{err}");
        assert!(err.contains("16"), "{err}");

        fs::write(ws.join("bin.dat"), [0_u8, 1, 2, 3, 255]).unwrap();
        let err = WorkspaceReadFileTool::new(&ws)
            .execute(serde_json::json!({"path": "bin.dat"}))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("二进制"), "{err}");
        let _ = fs::remove_dir_all(&ws);
    }

    #[tokio::test]
    async fn list_dir_basic_and_nested() {
        let ws = unique_temp("jiaclaw_list_dir_ok");
        fs::write(ws.join("MEMORY.md"), "mem").unwrap();
        fs::create_dir_all(ws.join("notes")).unwrap();
        fs::write(ws.join("notes").join("a.md"), "a").unwrap();
        let tool = WorkspaceListDirTool::new(&ws);
        assert_eq!(tool.name(), "list_dir");

        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.contains("MEMORY.md"), "{result}");
        assert!(result.contains("notes"), "{result}");
        assert!(result.contains("\"type\": \"file\""), "{result}");
        assert!(result.contains("\"type\": \"dir\""), "{result}");
        assert!(
            !result.contains("a.md"),
            "默认不递归不应列出子目录文件: {result}"
        );

        let nested = tool
            .execute(serde_json::json!({"path": "notes"}))
            .await
            .unwrap();
        assert!(nested.contains("a.md"), "{nested}");
        let _ = fs::remove_dir_all(&ws);
    }

    #[tokio::test]
    async fn list_dir_rejects_traversal() {
        let ws = unique_temp("jiaclaw_list_dir_trav");
        fs::create_dir_all(&ws).unwrap();
        let tool = WorkspaceListDirTool::new(&ws);
        let err = tool
            .execute(serde_json::json!({"path": "../"}))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("穿越") || err.contains("安全"), "{err}");

        let err = tool
            .execute(serde_json::json!({"path": "/tmp"}))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("绝对路径"), "{err}");
        let _ = fs::remove_dir_all(&ws);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn list_dir_rejects_symlink_escape() {
        let ws = unique_temp("jiaclaw_list_dir_symlink");
        let outside = ws.parent().unwrap().join(format!(
            "jiaclaw_list_outside_{}",
            ws.file_name().unwrap().to_string_lossy()
        ));
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), "nope").unwrap();
        std::os::unix::fs::symlink(&outside, ws.join("escape")).unwrap();

        let tool = WorkspaceListDirTool::new(&ws);
        let result = tool.execute(serde_json::json!({"path": "escape"})).await;
        assert!(result.is_err(), "symlink 逃逸应被拒绝: {result:?}");
        let _ = fs::remove_dir_all(&outside);
        let _ = fs::remove_dir_all(&ws);
    }

    #[tokio::test]
    async fn list_dir_truncates_at_max_entries() {
        let ws = unique_temp("jiaclaw_list_dir_max");
        for i in 0..5 {
            fs::write(ws.join(format!("f{i}.txt")), "x").unwrap();
        }
        let tool = WorkspaceListDirTool::new(&ws);
        let result = tool
            .execute(serde_json::json!({"max_entries": 2}))
            .await
            .unwrap();
        assert!(result.contains("\"truncated\": true"), "{result}");
        let parsed: ListDirOutput = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.entries.len(), 2);
        let _ = fs::remove_dir_all(&ws);
    }
}
