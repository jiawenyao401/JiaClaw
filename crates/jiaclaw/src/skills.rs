// Copyright 2026 JiaClaw contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! 技能发现和管理

use jiaclaw_core::JiaClawError;
use std::path::{Path, PathBuf};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

/// 技能定义
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Skill {
    /// 技能名称
    pub name: String,

    /// 技能描述
    pub description: String,

    /// 技能路径
    #[serde(skip)]
    pub path: PathBuf,

    /// 完整的 SKILL.md 内容（不含 frontmatter）
    #[serde(skip)]
    pub content: String,

    /// 触发关键词列表（可选）
    #[serde(default)]
    pub triggers: Vec<String>,
}

/// YAML frontmatter 结构
#[derive(Debug, Clone, serde::Deserialize)]
struct SkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    triggers: Vec<String>,
}

impl Skill {
    /// 从 SKILL.md 文件解析技能
    ///
    /// # Errors
    ///
    /// 如果技能文件不存在或无法读取，返回错误。
    pub fn from_file(skill_dir: &Path) -> Result<Self, JiaClawError> {
        let skill_file = skill_dir.join("SKILL.md");

        if !skill_file.exists() {
            return Err(JiaClawError::Configuration(format!(
                "技能文件不存在: {}",
                skill_file.display()
            )));
        }

        let raw_content = std::fs::read_to_string(&skill_file).map_err(|e| {
            JiaClawError::Configuration(format!("无法读取技能文件 {}: {e}", skill_file.display()))
        })?;

        let dir_name = skill_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let (frontmatter, content) = Self::parse_frontmatter(&raw_content)?;

        let name = frontmatter.name.unwrap_or(dir_name);
        let description = frontmatter
            .description
            .unwrap_or_else(|| Self::extract_description(&content));
        let triggers = frontmatter.triggers;

        Ok(Self {
            name,
            description,
            path: skill_dir.to_path_buf(),
            content,
            triggers,
        })
    }

    /// 解析 YAML frontmatter
    ///
    /// 返回 `(frontmatter, content_without_frontmatter)`
    fn parse_frontmatter(content: &str) -> Result<(SkillFrontmatter, String), JiaClawError> {
        let trimmed = content.trim_start();

        if !trimmed.starts_with("---") {
            return Ok((
                SkillFrontmatter {
                    name: None,
                    description: None,
                    triggers: Vec::new(),
                },
                content.to_string(),
            ));
        }

        let after_first_delimiter = &trimmed[3..];

        if let Some(end_pos) = after_first_delimiter.find("\n---") {
            let yaml_content = &after_first_delimiter[..end_pos];
            let remaining_content = &after_first_delimiter[end_pos + 4..];

            let frontmatter: SkillFrontmatter =
                serde_yaml::from_str(yaml_content).map_err(|e| {
                    JiaClawError::Configuration(format!("无法解析 YAML frontmatter: {e}"))
                })?;

            Ok((frontmatter, remaining_content.trim().to_string()))
        } else {
            Ok((
                SkillFrontmatter {
                    name: None,
                    description: None,
                    triggers: Vec::new(),
                },
                content.to_string(),
            ))
        }
    }

    /// 从 SKILL.md 内容中提取描述
    fn extract_description(content: &str) -> String {
        // 查找 ## Description 部分
        let lines: Vec<&str> = content.lines().collect();
        let mut in_description = false;
        let mut description_lines = Vec::new();

        for line in lines {
            let trimmed = line.trim();

            if trimmed.starts_with("## Description") {
                in_description = true;
                continue;
            }

            if in_description {
                if trimmed.starts_with("##") {
                    // 遇到下一个章节，结束
                    break;
                }
                if !trimmed.is_empty() {
                    description_lines.push(trimmed);
                }
            }
        }

        if description_lines.is_empty() {
            // 回退：使用第一段非空内容
            content
                .lines()
                .find(|line| !line.trim().is_empty() && !line.trim().starts_with('#'))
                .unwrap_or("无描述")
                .to_string()
        } else {
            description_lines.join(" ")
        }
    }

    /// 生成简短摘要（用于注入系统提示）
    #[must_use]
    pub fn summary(&self) -> String {
        format!("**{}**: {}", self.name, self.description)
    }
}

/// 技能发现器
pub struct SkillDiscovery {
    /// 技能根目录
    skills_root: PathBuf,
    /// 是否启用自动触发器激活
    auto_trigger_enabled: bool,
}

impl SkillDiscovery {
    /// 创建新的技能发现器
    #[must_use]
    pub fn new(workspace_path: &Path) -> Self {
        Self {
            skills_root: workspace_path.join("skills"),
            auto_trigger_enabled: true,
        }
    }

    /// 设置是否启用自动触发器激活
    #[must_use]
    pub fn with_auto_trigger(mut self, enabled: bool) -> Self {
        self.auto_trigger_enabled = enabled;
        self
    }

    /// 列出 `skills/` 下的一级子目录。目录不存在时返回空列表。
    fn skill_directories(&self) -> Result<Vec<PathBuf>, JiaClawError> {
        if !self.skills_root.exists() {
            tracing::debug!("技能目录不存在: {}", self.skills_root.display());
            return Ok(Vec::new());
        }

        let entries = std::fs::read_dir(&self.skills_root)
            .map_err(|e| JiaClawError::Configuration(format!("无法读取技能目录: {e}")))?;

        let mut dirs = Vec::new();
        for entry in entries {
            let entry =
                entry.map_err(|e| JiaClawError::Configuration(format!("无法读取目录条目: {e}")))?;
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            }
        }
        Ok(dirs)
    }

    /// 发现所有技能（启动时宽松模式：单个坏文件跳过并 warn）。
    ///
    /// # Errors
    ///
    /// 如果无法读取技能目录，返回错误。
    pub fn discover(&self) -> Result<Vec<Skill>, JiaClawError> {
        let mut skills = Vec::new();

        for path in self.skill_directories()? {
            match Skill::from_file(&path) {
                Ok(skill) => {
                    tracing::debug!("发现技能: {}", skill.name);
                    skills.push(skill);
                }
                Err(e) => {
                    tracing::warn!("跳过无效技能目录 {}: {e}", path.display());
                }
            }
        }

        Ok(skills)
    }

    /// 严格扫描：任一现存 `SKILL.md` 无法读取或解析则失败（热加载用）。
    ///
    /// 缺少 `SKILL.md` 的子目录会被跳过（不是技能）。目录本身无法读取时返回错误。
    ///
    /// # Errors
    ///
    /// 无法读取 `skills/`，或至少一个技能文件无效。
    pub fn discover_strict(&self) -> Result<Vec<Skill>, JiaClawError> {
        let mut skills = Vec::new();
        let mut errors = Vec::new();

        for path in self.skill_directories()? {
            let skill_file = path.join("SKILL.md");
            if !skill_file.exists() {
                tracing::debug!("跳过无 SKILL.md 的目录: {}", path.display());
                continue;
            }
            match Skill::from_file(&path) {
                Ok(skill) => {
                    tracing::debug!("发现技能: {}", skill.name);
                    skills.push(skill);
                }
                Err(e) => {
                    errors.push(format!("{}: {e}", path.display()));
                }
            }
        }

        if !errors.is_empty() {
            return Err(JiaClawError::Configuration(format!(
                "存在无效技能文件: {}",
                errors.join("; ")
            )));
        }

        Ok(skills)
    }

    /// 查找特定技能
    ///
    /// # Errors
    ///
    /// 如果技能文件存在但无法解析，返回错误。
    pub fn find(&self, skill_name: &str) -> Result<Option<Skill>, JiaClawError> {
        let skill_dir = self.skills_root.join(skill_name);

        if !skill_dir.exists() {
            return Ok(None);
        }

        Skill::from_file(&skill_dir).map(Some)
    }

    /// 根据用户消息自动查找应该激活的技能
    ///
    /// 返回所有触发器匹配的技能名称列表
    pub fn auto_trigger_skills(
        &self,
        user_message: &str,
        discovered_skills: &[Skill],
    ) -> Vec<String> {
        if !self.auto_trigger_enabled {
            return Vec::new();
        }

        let message_lower = user_message.to_lowercase();
        let mut triggered = Vec::new();

        for skill in discovered_skills {
            if skill.triggers.is_empty() {
                continue;
            }

            for trigger in &skill.triggers {
                let trigger_lower = trigger.to_lowercase();
                if message_lower.contains(&trigger_lower) {
                    triggered.push(skill.name.clone());
                    tracing::debug!("技能 '{}' 由触发词 '{}' 自动激活", skill.name, trigger);
                    break;
                }
            }
        }

        triggered
    }
}

/// 进程内技能注册表：短读锁快照，热加载时短写锁替换。
#[derive(Debug)]
pub struct SkillRegistry {
    inner: RwLock<Vec<Skill>>,
}

impl SkillRegistry {
    /// 用已扫描的技能列表构造注册表。
    #[must_use]
    pub fn new(skills: Vec<Skill>) -> Self {
        Self {
            inner: RwLock::new(skills),
        }
    }

    fn lock_read(&self) -> RwLockReadGuard<'_, Vec<Skill>> {
        self.inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn lock_write(&self) -> RwLockWriteGuard<'_, Vec<Skill>> {
        self.inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// 克隆当前技能列表（持锁时间短，调用方随后不再持锁）。
    #[must_use]
    pub fn snapshot(&self) -> Vec<Skill> {
        self.lock_read().clone()
    }

    /// 重新扫描工作区 `skills/` 并替换注册表。
    ///
    /// 磁盘扫描在锁外完成；仅成功后短时间持写锁替换。失败时保留旧表。
    ///
    /// # Errors
    ///
    /// 目录无法读取，或任一 `SKILL.md` 无效。此时注册表内容不变。
    pub fn reload(&self, workspace_path: &Path) -> Result<Vec<Skill>, JiaClawError> {
        let new_skills = SkillDiscovery::new(workspace_path).discover_strict()?;
        {
            let mut guard = self.lock_write();
            guard.clone_from(&new_skills);
        }
        Ok(new_skills)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_skill_from_file() {
        let temp_dir = std::env::temp_dir().join("jiaclaw_test_skill");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let skill_content = r"# Test Skill

## Description

This is a test skill for testing purposes.

## Tools

- test_tool

## Usage

Use this for testing.
";

        fs::write(temp_dir.join("SKILL.md"), skill_content).unwrap();

        let skill = Skill::from_file(&temp_dir).unwrap();
        assert_eq!(skill.name, "jiaclaw_test_skill");
        assert!(skill.description.contains("test skill"));
        assert!(skill.triggers.is_empty());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_skill_with_frontmatter() {
        let temp_dir = std::env::temp_dir().join("jiaclaw_test_frontmatter");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let skill_content = r"---
name: web_search
description: 搜索互联网信息
triggers:
  - search
  - 搜索
  - find
---

# Web Search Skill

这是一个网络搜索技能。

## Usage

使用此技能搜索网络信息。
";

        fs::write(temp_dir.join("SKILL.md"), skill_content).unwrap();

        let skill = Skill::from_file(&temp_dir).unwrap();
        assert_eq!(skill.name, "web_search");
        assert_eq!(skill.description, "搜索互联网信息");
        assert_eq!(skill.triggers, vec!["search", "搜索", "find"]);
        assert!(skill.content.contains("这是一个网络搜索技能"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_skill_discovery() {
        let temp_workspace = std::env::temp_dir().join("jiaclaw_test_discovery");
        let _ = fs::remove_dir_all(&temp_workspace);

        let skills_dir = temp_workspace.join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        // 创建两个测试技能
        let skill1_dir = skills_dir.join("skill1");
        fs::create_dir_all(&skill1_dir).unwrap();
        fs::write(
            skill1_dir.join("SKILL.md"),
            "# Skill 1\n\n## Description\n\nFirst skill",
        )
        .unwrap();

        let skill2_dir = skills_dir.join("skill2");
        fs::create_dir_all(&skill2_dir).unwrap();
        fs::write(
            skill2_dir.join("SKILL.md"),
            "# Skill 2\n\n## Description\n\nSecond skill",
        )
        .unwrap();

        let discovery = SkillDiscovery::new(&temp_workspace);
        let skills = discovery.discover().unwrap();

        assert_eq!(skills.len(), 2);
        assert!(skills.iter().any(|s| s.name == "skill1"));
        assert!(skills.iter().any(|s| s.name == "skill2"));

        let _ = fs::remove_dir_all(&temp_workspace);
    }

    #[test]
    fn test_skill_find() {
        let temp_workspace = std::env::temp_dir().join("jiaclaw_test_find");
        let _ = fs::remove_dir_all(&temp_workspace);

        let skills_dir = temp_workspace.join("skills");
        let skill_dir = skills_dir.join("findme");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "# Find Me\n\n## Description\n\nTest",
        )
        .unwrap();

        let discovery = SkillDiscovery::new(&temp_workspace);

        let found = discovery.find("findme").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "findme");

        let not_found = discovery.find("notexist").unwrap();
        assert!(not_found.is_none());

        let _ = fs::remove_dir_all(&temp_workspace);
    }

    #[test]
    fn test_auto_trigger_skills() {
        let temp_workspace = std::env::temp_dir().join("jiaclaw_test_trigger");
        let _ = fs::remove_dir_all(&temp_workspace);

        let skills_dir = temp_workspace.join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let search_skill = r"---
name: web_search
description: 搜索互联网
triggers:
  - search
  - 搜索
---
# Web Search
";
        let search_dir = skills_dir.join("web_search");
        fs::create_dir_all(&search_dir).unwrap();
        fs::write(search_dir.join("SKILL.md"), search_skill).unwrap();

        let calc_skill = r"---
name: calculator
description: 计算器
triggers:
  - calculate
  - 计算
---
# Calculator
";
        let calc_dir = skills_dir.join("calculator");
        fs::create_dir_all(&calc_dir).unwrap();
        fs::write(calc_dir.join("SKILL.md"), calc_skill).unwrap();

        let discovery = SkillDiscovery::new(&temp_workspace);
        let skills = discovery.discover().unwrap();

        let triggered = discovery.auto_trigger_skills("请帮我搜索一下", &skills);
        assert_eq!(triggered.len(), 1);
        assert!(triggered.contains(&"web_search".to_string()));

        let triggered = discovery.auto_trigger_skills("help me search and calculate", &skills);
        assert_eq!(triggered.len(), 2);
        assert!(triggered.contains(&"web_search".to_string()));
        assert!(triggered.contains(&"calculator".to_string()));

        let triggered = discovery.auto_trigger_skills("hello world", &skills);
        assert_eq!(triggered.len(), 0);

        let _ = fs::remove_dir_all(&temp_workspace);
    }

    #[test]
    fn test_auto_trigger_disabled() {
        let temp_workspace = std::env::temp_dir().join("jiaclaw_test_trigger_disabled");
        let _ = fs::remove_dir_all(&temp_workspace);

        let skills_dir = temp_workspace.join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let search_skill = r"---
name: web_search
triggers:
  - search
---
# Web Search
";
        let search_dir = skills_dir.join("web_search");
        fs::create_dir_all(&search_dir).unwrap();
        fs::write(search_dir.join("SKILL.md"), search_skill).unwrap();

        let discovery = SkillDiscovery::new(&temp_workspace).with_auto_trigger(false);
        let skills = discovery.discover().unwrap();

        let triggered = discovery.auto_trigger_skills("search for something", &skills);
        assert_eq!(triggered.len(), 0);

        let _ = fs::remove_dir_all(&temp_workspace);
    }

    fn unique_workspace(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}_{}_{nanos}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_skill(workspace: &Path, name: &str, body: &str) {
        let dir = workspace.join("skills").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), body).unwrap();
    }

    #[test]
    fn discover_strict_loads_valid_skills() {
        let workspace = unique_workspace("jiaclaw_strict_ok");
        write_skill(
            &workspace,
            "alpha",
            "# Alpha\n\n## Description\n\nFirst skill\n",
        );

        let skills = SkillDiscovery::new(&workspace).discover_strict().unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "alpha");

        let _ = fs::remove_dir_all(&workspace);
    }

    #[test]
    fn discover_strict_fails_on_bad_skill_file() {
        let workspace = unique_workspace("jiaclaw_strict_bad");
        write_skill(
            &workspace,
            "alpha",
            "# Alpha\n\n## Description\n\nFirst skill\n",
        );
        write_skill(
            &workspace,
            "broken",
            "---\nname: [not yaml\n---\n# Broken\n",
        );

        let err = SkillDiscovery::new(&workspace)
            .discover_strict()
            .expect_err("坏文件应使严格扫描失败");
        assert!(
            err.to_string().contains("无效技能"),
            "错误应说明无效技能，实际: {err}"
        );

        let _ = fs::remove_dir_all(&workspace);
    }

    #[test]
    fn registry_reload_replaces_table() {
        let workspace = unique_workspace("jiaclaw_registry_reload");
        write_skill(
            &workspace,
            "alpha",
            "# Alpha\n\n## Description\n\nFirst skill\n",
        );
        let registry = SkillRegistry::new(
            SkillDiscovery::new(&workspace)
                .discover()
                .expect("初始扫描"),
        );
        assert_eq!(registry.snapshot().len(), 1);

        write_skill(
            &workspace,
            "beta",
            "# Beta\n\n## Description\n\nSecond skill\n",
        );
        let reloaded = registry.reload(&workspace).expect("重载应成功");
        assert_eq!(reloaded.len(), 2);
        let names: Vec<_> = registry.snapshot().iter().map(|s| s.name.clone()).collect();
        assert!(names.contains(&"alpha".to_string()));
        assert!(names.contains(&"beta".to_string()));

        let _ = fs::remove_dir_all(&workspace);
    }

    #[test]
    fn registry_reload_keeps_old_table_on_bad_file() {
        let workspace = unique_workspace("jiaclaw_registry_keep");
        write_skill(
            &workspace,
            "alpha",
            "# Alpha\n\n## Description\n\nFirst skill\n",
        );
        let registry = SkillRegistry::new(
            SkillDiscovery::new(&workspace)
                .discover()
                .expect("初始扫描"),
        );

        write_skill(
            &workspace,
            "broken",
            "---\nname: [not yaml\n---\n# Broken\n",
        );
        let err = registry.reload(&workspace).expect_err("坏文件应使重载失败");
        assert!(
            err.to_string().contains("无效技能"),
            "错误应说明无效技能，实际: {err}"
        );

        let names: Vec<_> = registry.snapshot().iter().map(|s| s.name.clone()).collect();
        assert_eq!(names, vec!["alpha".to_string()]);

        let _ = fs::remove_dir_all(&workspace);
    }
}
