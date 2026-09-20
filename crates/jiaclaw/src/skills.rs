// Copyright 2026 JiaClaw contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! 技能发现和管理

use jiaclaw_core::JiaClawError;
use std::path::{Path, PathBuf};

/// 技能定义
#[derive(Debug, Clone)]
pub struct Skill {
    /// 技能名称
    pub name: String,

    /// 技能描述
    pub description: String,

    /// 技能路径
    pub path: PathBuf,

    /// 完整的 SKILL.md 内容
    pub content: String,
}

impl Skill {
    /// 从 SKILL.md 文件解析技能
    pub fn from_file(skill_dir: &Path) -> Result<Self, JiaClawError> {
        let skill_file = skill_dir.join("SKILL.md");

        if !skill_file.exists() {
            return Err(JiaClawError::Configuration(format!(
                "技能文件不存在: {}",
                skill_file.display()
            )));
        }

        let content = std::fs::read_to_string(&skill_file).map_err(|e| {
            JiaClawError::Configuration(format!("无法读取技能文件 {}: {e}", skill_file.display()))
        })?;

        let name = skill_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let description = Self::extract_description(&content);

        Ok(Self {
            name,
            description,
            path: skill_dir.to_path_buf(),
            content,
        })
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
    pub fn summary(&self) -> String {
        format!("**{}**: {}", self.name, self.description)
    }
}

/// 技能发现器
pub struct SkillDiscovery {
    /// 技能根目录
    skills_root: PathBuf,
}

impl SkillDiscovery {
    /// 创建新的技能发现器
    pub fn new(workspace_path: &Path) -> Self {
        Self {
            skills_root: workspace_path.join("skills"),
        }
    }

    /// 发现所有技能
    pub fn discover(&self) -> Result<Vec<Skill>, JiaClawError> {
        if !self.skills_root.exists() {
            tracing::debug!("技能目录不存在: {}", self.skills_root.display());
            return Ok(Vec::new());
        }

        let mut skills = Vec::new();

        let entries = std::fs::read_dir(&self.skills_root)
            .map_err(|e| JiaClawError::Configuration(format!("无法读取技能目录: {e}")))?;

        for entry in entries {
            let entry =
                entry.map_err(|e| JiaClawError::Configuration(format!("无法读取目录条目: {e}")))?;

            let path = entry.path();

            if path.is_dir() {
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
        }

        Ok(skills)
    }

    /// 查找特定技能
    pub fn find(&self, skill_name: &str) -> Result<Option<Skill>, JiaClawError> {
        let skill_dir = self.skills_root.join(skill_name);

        if !skill_dir.exists() {
            return Ok(None);
        }

        Skill::from_file(&skill_dir).map(Some)
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
}
