// Copyright 2026 JiaClaw contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `JiaClaw` 可执行宿主

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use jiaclaw::JiaClawAgent;
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
        Commands::Serve { config, bind } => {
            serve_command(config, bind).await?;
        }
        Commands::Chat { config, message } => {
            chat_command(config, &message)?;
        }
        Commands::Version => {
            version_command();
        }
    }

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

fn chat_command(config_path: Option<PathBuf>, message: &str) -> Result<()> {
    tracing::info!("运行单次聊天");

    let config = if let Some(path) = config_path {
        let path_str = path.to_string_lossy();
        if path_str.ends_with(".toml") {
            AgentConfig::from_toml_file(&path)?
        } else if path_str.ends_with(".json") {
            AgentConfig::from_json_file(&path)?
        } else {
            // 尝试两种格式
            AgentConfig::from_toml_file(&path)
                .or_else(|_| AgentConfig::from_json_file(&path))?
        }
    } else {
        AgentConfig::default()
    };

    tracing::info!("使用 Agent 配置: {}", config.name);

    // 创建并使用 agent（通过 jiaclaw facade）
    // 注意：由于 StateKnot 处于 pre-alpha 状态，这是一个存根实现
    let agent = JiaClawAgent::new(config)
        .context("创建 JiaClawAgent 失败")?;

    let request = ChatRequest {
        messages: vec![ChatMessage {
            role: MessageRole::User,
            content: message.to_string(),
        }],
        enabled_tools: vec![],
        enabled_skills: vec![],
    };

    tracing::info!("用户消息: {}", message);

    let response = agent
        .chat(&request)
        .context("聊天请求失败")?;

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
