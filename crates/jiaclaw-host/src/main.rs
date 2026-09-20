// Copyright 2026 JiaClaw contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `JiaClaw` 可执行宿主

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use jiaclaw::{JiaClawAgent, Workspace};
use jiaclaw_core::{AgentConfig, ChatMessage, ChatRequest, MessageRole};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "jiaclaw")]
#[command(about = "JiaClaw - 基于 StateKnot 的个人持久化智能体运行时", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 初始化工作空间和配置
    Init {
        /// 工作空间路径
        #[arg(short, long, value_name = "PATH")]
        path: Option<PathBuf>,

        /// 强制覆盖已存在的文件
        #[arg(short, long)]
        force: bool,
    },

    /// 启动 `JiaClaw` Agent 服务
    Serve {
        /// 配置文件路径
        #[arg(short, long, value_name = "FILE")]
        config: Option<PathBuf>,

        /// 绑定地址
        #[arg(short, long, default_value = "127.0.0.1:8080")]
        bind: String,
    },

    /// 运行单次聊天（用于测试）
    Chat {
        /// 配置文件路径
        #[arg(short, long, value_name = "FILE")]
        config: Option<PathBuf>,

        /// 用户消息
        #[arg(value_name = "MESSAGE")]
        message: String,
    },

    /// 显示版本和构建信息
    Version,

    /// 检查配置和连接状态
    Doctor {
        /// 配置文件路径
        #[arg(short, long, value_name = "FILE")]
        config: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init { path, force } => {
            init_command(path, force)?;
        }
        Commands::Serve { config, bind } => {
            serve_command(config, bind).await?;
        }
        Commands::Chat { config, message } => {
            chat_command(config, &message).await?;
        }
        Commands::Version => {
            version_command();
        }
        Commands::Doctor { config } => {
            doctor_command(config)?;
        }
    }

    Ok(())
}

fn init_command(path: Option<PathBuf>, force: bool) -> Result<()> {
    let workspace_path = path.unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".jiaclaw")
            .join("workspace")
    });

    tracing::info!("初始化工作空间: {}", workspace_path.display());

    // 检查是否已存在
    if workspace_path.exists() && !force {
        tracing::warn!("工作空间已存在。使用 --force 强制覆盖。");
        println!("\n❌ 工作空间已存在: {}", workspace_path.display());
        println!("   使用 --force 标志强制覆盖现有文件。");
        return Ok(());
    }

    // 初始化工作空间
    Workspace::init(&workspace_path).context("初始化工作空间失败")?;

    println!("\n✅ 工作空间已初始化: {}", workspace_path.display());
    println!("\n📁 已创建文件:");
    println!("   • AGENTS.md  - Agent 配置和元数据");
    println!("   • SOUL.md    - Agent 性格和指令");
    println!("   • USER.md    - 用户信息和偏好");
    println!("   • MEMORY.md  - 长期记忆和上下文");
    println!("   • skills/    - 技能目录");
    println!("     ├── search/SKILL.md");
    println!("     └── calculator/SKILL.md");

    println!("\n📝 下一步:");
    println!("   1. 编辑工作空间文件以个性化你的 Agent");
    println!("   2. 配置 API key（可选）:");
    println!("      export JIACLAW_API_KEY=your-key-here");
    println!("   3. 开始聊天:");
    println!("      jiaclaw chat \"你好\"");

    println!("\n💡 提示:");
    println!("   • 无 API key 时将使用存根模式（演示功能）");
    println!("   • 参见 config/jiaclaw.toml.example 了解完整配置选项");

    Ok(())
}

async fn serve_command(_config: Option<PathBuf>, bind: String) -> Result<()> {
    tracing::info!("正在启动 JiaClaw Agent 服务于 {}", bind);

    tracing::warn!(
        "服务模式尚未完全实现。StateKnot AgentHost 和 HTTP 服务需要：\n\
         - 稳定的 AgentHost API\n\
         - PostgreSQL 连接配置\n\
         - 身份验证和授权集成\n\
         参见 docs/stateknot-gaps.md 了解详细信息。"
    );

    tracing::info!("服务存根已创建。按 Ctrl+C 退出。");

    // 等待 Ctrl+C
    tokio::signal::ctrl_c()
        .await
        .context("等待 Ctrl+C 信号失败")?;

    tracing::info!("正在关闭...");
    Ok(())
}

async fn chat_command(config_path: Option<PathBuf>, message: &str) -> Result<()> {
    tracing::info!("运行单次聊天");

    let config = if let Some(path) = config_path {
        let path_str = path.to_string_lossy();
        if path_str.ends_with(".toml") {
            AgentConfig::from_toml_file(&path)?
        } else if path_str.ends_with(".json") {
            AgentConfig::from_json_file(&path)?
        } else {
            // 尝试两种格式
            AgentConfig::from_toml_file(&path).or_else(|_| AgentConfig::from_json_file(&path))?
        }
    } else {
        AgentConfig::default()
    };

    tracing::info!("使用 Agent 配置: {}", config.name);

    // 检查工作空间是否存在
    if !config.workspace_path.exists() {
        tracing::warn!("工作空间不存在，使用默认配置");
        println!("\n💡 提示: 运行 'jiaclaw init' 创建工作空间");
    }

    // 创建并使用 agent
    let agent = JiaClawAgent::new(config).context("创建 JiaClawAgent 失败")?;

    let request = ChatRequest {
        messages: vec![ChatMessage {
            role: MessageRole::User,
            content: message.to_string(),
        }],
        enabled_tools: vec![],
        enabled_skills: vec![],
    };

    tracing::info!("用户消息: {}", message);

    let response = agent.chat(&request).await.context("聊天请求失败")?;

    // 显示工具调用（如果有）
    if !response.tool_calls.is_empty() {
        println!("\n🔧 工具调用:");
        for tool_call in &response.tool_calls {
            println!("   • {}", tool_call.tool_name);
            if let Some(ref result) = tool_call.result {
                if let Some(result_str) = result.as_str() {
                    // 结果是字符串，直接显示
                    println!("     结果: {}", result_str.lines().take(3).collect::<Vec<_>>().join("\n     "));
                    if result_str.lines().count() > 3 {
                        println!("     ...");
                    }
                } else {
                    // 结果是其他 JSON，格式化显示
                    println!("     结果: {}", serde_json::to_string_pretty(result).unwrap_or_default());
                }
            }
        }
        println!();
    }

    println!("\n助手回复:");
    println!("{}", response.message.content);
    println!("\n状态: {:?}", response.status);

    Ok(())
}

fn version_command() {
    println!("JiaClaw v{}", env!("CARGO_PKG_VERSION"));
    println!("基于 StateKnot 框架构建");
    println!("许可证: Apache-2.0 OR MIT");
    println!("仓库: https://github.com/jiawenyao401/JiaClaw");
}

#[allow(clippy::too_many_lines)]
fn doctor_command(config_path: Option<PathBuf>) -> Result<()> {
    println!("🔍 JiaClaw 配置检查\n");

    let config = if let Some(path) = config_path {
        let path_str = path.to_string_lossy();
        if path_str.ends_with(".toml") {
            AgentConfig::from_toml_file(&path)?
        } else if path_str.ends_with(".json") {
            AgentConfig::from_json_file(&path)?
        } else {
            AgentConfig::from_toml_file(&path).or_else(|_| AgentConfig::from_json_file(&path))?
        }
    } else {
        AgentConfig::default()
    };

    // 1. 检查工作空间
    println!("📁 工作空间检查");
    println!("   路径: {}", config.workspace_path.display());

    if config.workspace_path.exists() {
        println!("   状态: ✅ 存在");

        let workspace = Workspace::load(&config.workspace_path)?;
        let mut files = Vec::new();
        if workspace.agents.is_some() {
            files.push("AGENTS.md");
        }
        if workspace.soul.is_some() {
            files.push("SOUL.md");
        }
        if workspace.user.is_some() {
            files.push("USER.md");
        }
        if workspace.memory.is_some() {
            files.push("MEMORY.md");
        }

        if files.is_empty() {
            println!("   ⚠️  没有找到工作空间文件");
            println!("   💡 运行 'jiaclaw init' 创建默认文件");
        } else {
            println!("   文件: {} 个已加载 ({})", files.len(), files.join(", "));
        }

        // 检查技能
        let skills_dir = config.workspace_path.join("skills");
        if skills_dir.exists() {
            let discovery = jiaclaw::SkillDiscovery::new(&config.workspace_path);
            match discovery.discover() {
                Ok(skills) => {
                    if skills.is_empty() {
                        println!("   技能: 0 个");
                    } else {
                        println!("   技能: {} 个发现", skills.len());
                        for skill in &skills {
                            println!("         • {}", skill.name);
                        }
                    }
                }
                Err(e) => {
                    println!("   ⚠️  技能发现失败: {e}");
                }
            }
        } else {
            println!("   技能: 目录不存在");
        }
    } else {
        println!("   状态: ❌ 不存在");
        println!("   💡 运行 'jiaclaw init' 创建工作空间");
    }

    // 2. 检查提供商配置
    println!("\n🔌 提供商配置");
    println!("   类型: {}", config.provider.provider_type);
    println!("   模型: {}", config.provider.model);
    println!("   Base URL: {}", config.provider.base_url);

    // 检查 API key
    let env_key = std::env::var("JIACLAW_API_KEY").ok();
    let has_key = config.provider.api_key.is_some() || env_key.is_some();

    if has_key {
        println!("   API Key: ✅ 已配置");

        if config.provider.provider_type == "brokerrouter" {
            println!("\n   🔍 Brokerrouter 连接测试");
            println!("      注意: 完整的连接测试需要有效的虚拟密钥");
            println!("      当前仅进行配置验证");

            // 简单的 URL 格式检查
            if config.provider.base_url.starts_with("http://")
                || config.provider.base_url.starts_with("https://")
            {
                println!("      Base URL: ✅ 格式有效");
            } else {
                println!("      Base URL: ⚠️  格式可能无效（应以 http:// 或 https:// 开头）");
            }

            // 检查虚拟密钥格式
            let key = config.provider.api_key.as_deref().or(env_key.as_deref());
            if let Some(k) = key {
                if k.starts_with("brk_") {
                    println!("      Virtual Key: ✅ 格式正确（brk_ 前缀）");
                } else {
                    println!("      Virtual Key: ⚠️  格式可能不正确（应以 brk_ 开头）");
                }
            }
        }
    } else {
        println!("   API Key: ⚠️  未配置");
        println!("   💡 将使用存根模式（演示功能）");
        println!("   💡 设置环境变量: export JIACLAW_API_KEY=your-key");
    }

    // 3. 工具系统
    println!("\n🔧 工具系统");
    let agent = JiaClawAgent::new(config.clone())?;
    let tool_list = agent.tools().list();
    println!("   本地工具: {} 个已注册", tool_list.len());
    for tool_name in tool_list {
        if let Some(tool) = agent.tools().get(tool_name) {
            println!("      • {}: {}", tool.name(), tool.description());
        }
    }

    // 4. StateKnot 集成状态
    println!("\n⚙️  StateKnot 集成");
    println!("   状态: ⏳ 等待稳定 API 发布");
    println!("   持久化: ❌ 未启用");
    println!("   PostgreSQL: ❌ 未配置");
    println!("   💡 参见 docs/stateknot-gaps.md 了解详情");

    // 5. 总结
    println!("\n📊 总结");
    if config.workspace_path.exists() && has_key {
        println!("   ✅ 配置良好，可以开始使用");
        println!("   💡 试试: jiaclaw chat \"你好\"");
    } else if !config.workspace_path.exists() {
        println!("   ⚠️  需要初始化工作空间");
        println!("   💡 运行: jiaclaw init");
    } else if !has_key {
        println!("   ⚠️  未配置 API key，将使用存根模式");
        println!("   💡 设置: export JIACLAW_API_KEY=your-key");
        println!("   💡 或在存根模式下测试: jiaclaw chat \"你好\"");
    }

    Ok(())
}
