// Copyright 2026 JiaClaw contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `JiaClaw` 可执行宿主

use anyhow::{Context, Result};
use axum::{
    body::Bytes,
    extract::{Path, Query, Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{
        sse::{Event, Sse},
        IntoResponse, Json, Response,
    },
    routing::{get, post},
    Router,
};
use clap::{Parser, Subcommand};
use futures_util::{stream, Stream};
use governor::{
    clock::{Clock, DefaultClock},
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use hmac::{Hmac, Mac};
use jiaclaw::{
    inspect_heartbeat_file, inspect_identity_file, inspect_memory_file, load_heartbeat_message,
    resolve_heartbeat_path, JiaClawAgent, Workspace,
};
use jiaclaw_core::{
    AgentConfig, ChatMessage, ChatRequest, ChatResponse, MessageRole, ToolCall,
    MAX_MAX_TOOL_ITERATIONS, MAX_SESSION_MESSAGES, MIN_MAX_TOOL_ITERATIONS,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Sha256;
use std::{
    collections::HashMap,
    convert::Infallible,
    num::NonZeroU32,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::time::Instant;
use tower_http::cors::{Any, CorsLayer};

mod metrics;
use metrics::{classify_http_path, Metrics, PROMETHEUS_CONTENT_TYPE};

/// 进程内全局（非按 IP）速率限制器，oneshot 测试无需 `ConnectInfo`。
type GlobalRateLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// 请求追踪头。大小写不敏感，响应回写同名头。
const X_REQUEST_ID: &str = "x-request-id";

/// 会话 JSONL 导出的 `Content-Type`（NDJSON）。
const SESSION_EXPORT_NDJSON: &str = "application/x-ndjson";

/// Telegram Bot API webhook `secret_token` 请求头（官方名称，大小写不敏感）。
const X_TELEGRAM_BOT_API_SECRET_TOKEN: &str = "X-Telegram-Bot-Api-Secret-Token";

/// Telegram Bot API 默认根路径（不含 `/bot{token}`）。
const TELEGRAM_API_BASE: &str = "https://api.telegram.org";

/// Telegram `sendMessage` 文本上限（字符）。
const TELEGRAM_MAX_TEXT_LEN: usize = 4096;

/// Telegram 出站 HTTP 超时（秒）。
const TELEGRAM_SEND_TIMEOUT_SECS: u64 = 10;

/// Slack Events API 签名头（官方名称，大小写不敏感）。
const X_SLACK_SIGNATURE: &str = "X-Slack-Signature";

/// Slack Events API 请求时间戳头（Unix 秒）。
const X_SLACK_REQUEST_TIMESTAMP: &str = "X-Slack-Request-Timestamp";

/// Slack Web API 默认根路径。
const SLACK_API_BASE: &str = "https://slack.com/api";

/// Slack `chat.postMessage` 文本上限（字符）。
const SLACK_MAX_TEXT_LEN: usize = 40_000;

/// Slack 出站 HTTP 超时（秒）。
const SLACK_SEND_TIMEOUT_SECS: u64 = 10;

/// Slack 签名时间窗（秒）：`|now - timestamp|` 超过则拒绝。
const SLACK_MAX_TIMESTAMP_SKEW_SECS: u64 = 300;

/// Discord Interactions 签名头（官方名称，大小写不敏感）。
const X_SIGNATURE_ED25519: &str = "X-Signature-Ed25519";

/// Discord Interactions 请求时间戳头。
const X_SIGNATURE_TIMESTAMP: &str = "X-Signature-Timestamp";

/// Discord HTTP API 默认根路径（含 v10）。
const DISCORD_API_BASE: &str = "https://discord.com/api/v10";

/// Discord 消息内容上限（字符）。
const DISCORD_MAX_TEXT_LEN: usize = 2000;

/// Discord 出站 HTTP 超时（秒）。
const DISCORD_SEND_TIMEOUT_SECS: u64 = 10;

/// Discord Interaction type: PING。
const DISCORD_INTERACTION_PING: u64 = 1;

/// Discord Interaction type: `APPLICATION_COMMAND`。
const DISCORD_INTERACTION_APPLICATION_COMMAND: u64 = 2;

/// Discord application command type: `CHAT_INPUT`。
const DISCORD_COMMAND_CHAT_INPUT: u64 = 1;

/// Discord command option type: STRING。
const DISCORD_OPTION_STRING: u64 = 3;

/// Discord callback type: PONG。
const DISCORD_CALLBACK_PONG: u64 = 1;

/// Discord callback type: `CHANNEL_MESSAGE_WITH_SOURCE`。
const DISCORD_CALLBACK_CHANNEL_MESSAGE: u64 = 4;

/// Discord callback type: `DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE`。
const DISCORD_CALLBACK_DEFERRED_CHANNEL_MESSAGE: u64 = 5;

/// Discord message flag: EPHEMERAL。
const DISCORD_FLAG_EPHEMERAL: u64 = 64;

/// HMAC-SHA256 用于 Slack v0 签名。
type HmacSha256 = Hmac<Sha256>;

/// 手写 `OpenAPI` 3 草图（不引入代码生成）。
const OPENAPI_JSON: &str = include_str!("openapi.json");

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

        /// 绑定地址（覆盖配置文件）
        #[arg(short, long)]
        bind: Option<String>,
    },

    /// 运行聊天（支持单次消息或交互式 REPL）
    Chat {
        /// 配置文件路径
        #[arg(short, long, value_name = "FILE")]
        config: Option<PathBuf>,

        /// 用户消息（如果提供，则执行单次聊天；否则进入 REPL）
        #[arg(value_name = "MESSAGE")]
        message: Option<String>,

        /// 启用的技能列表（可重复使用）
        #[arg(short, long = "skill", value_name = "NAME")]
        skills: Vec<String>,

        /// 会话 ID（可选，用于恢复历史对话）
        #[arg(long, value_name = "ID")]
        session: Option<String>,

        /// 禁用技能自动激活
        #[arg(long)]
        no_auto_skill: bool,
    },

    /// 显示版本和构建信息
    Version,

    /// 检查配置和连接状态
    Doctor {
        /// 配置文件路径
        #[arg(short, long, value_name = "FILE")]
        config: Option<PathBuf>,
    },

    /// 列出已发现的技能
    Skills {
        /// 配置文件路径
        #[arg(short, long, value_name = "FILE")]
        config: Option<PathBuf>,

        /// 显示详细信息
        #[arg(short, long)]
        verbose: bool,
    },

    /// 工作区长期记忆（MEMORY.md）
    Memory {
        #[command(subcommand)]
        action: MemoryCommands,
    },

    /// Agent 人格（SOUL.md）
    Soul {
        #[command(subcommand)]
        action: IdentityFileCommands,
    },

    /// 用户画像（USER.md）
    User {
        #[command(subcommand)]
        action: IdentityFileCommands,
    },

    /// 会话（只读导出；不触发摘要、不改写 store）
    Session {
        #[command(subcommand)]
        action: SessionCommands,
    },
}

/// 会话子命令
#[derive(Subcommand)]
enum SessionCommands {
    /// 导出会话历史为 JSONL（默认 stdout）
    Export {
        /// 会话 ID
        id: String,

        /// 输出文件（省略则写 stdout）
        #[arg(short = 'o', long = "output", value_name = "FILE")]
        output: Option<PathBuf>,

        /// 配置文件路径
        #[arg(short, long, value_name = "FILE")]
        config: Option<PathBuf>,
    },
}

/// 长期记忆子命令
#[derive(Subcommand)]
enum MemoryCommands {
    /// 显示约定 MEMORY 文件内容
    Show {
        /// 配置文件路径
        #[arg(short, long, value_name = "FILE")]
        config: Option<PathBuf>,
    },
}

/// 人格 / 用户画像子命令
#[derive(Subcommand)]
enum IdentityFileCommands {
    /// 显示约定文件内容
    Show {
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
        Commands::Chat {
            config,
            message,
            skills,
            session,
            no_auto_skill,
        } => {
            chat_command(config, message.as_deref(), skills, session, no_auto_skill).await?;
        }
        Commands::Version => {
            version_command();
        }
        Commands::Doctor { config } => {
            doctor_command(config)?;
        }
        Commands::Skills { config, verbose } => {
            skills_command(config, verbose)?;
        }
        Commands::Memory { action } => match action {
            MemoryCommands::Show { config } => {
                memory_show_command(config)?;
            }
        },
        Commands::Soul { action } => match action {
            IdentityFileCommands::Show { config } => {
                identity_show_command(config, IdentityShowKind::Soul)?;
            }
        },
        Commands::User { action } => match action {
            IdentityFileCommands::Show { config } => {
                identity_show_command(config, IdentityShowKind::User)?;
            }
        },
        Commands::Session { action } => match action {
            SessionCommands::Export { id, output, config } => {
                session_export_command(config, &id, output.as_deref())?;
            }
        },
    }

    Ok(())
}

#[allow(clippy::too_many_lines)]
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
    println!("   • 可选 HEARTBEAT.md：仅 serve 进程可按间隔自检（默认关闭，见 [heartbeat]）");
    println!("   • 参见 config/jiaclaw.toml.example 了解完整配置选项");

    Ok(())
}

/// 会话闲置 TTL 后台扫描间隔。
const SESSION_TTL_SWEEP_INTERVAL: Duration = Duration::from_secs(30);

/// 内存中的一条会话：消息 + 最近触达时间。
struct SessionRecord {
    messages: Vec<ChatMessage>,
    last_accessed: Instant,
}

impl SessionRecord {
    fn new(messages: Vec<ChatMessage>) -> Self {
        Self {
            messages,
            last_accessed: Instant::now(),
        }
    }

    fn touch(&mut self) {
        self.last_accessed = Instant::now();
    }

    fn is_expired(&self, ttl: Duration, now: Instant) -> bool {
        now.saturating_duration_since(self.last_accessed) >= ttl
    }
}

/// HTTP 服务的共享状态
#[derive(Clone)]
struct AppState {
    agent: Arc<JiaClawAgent>,
    sessions: Arc<Mutex<HashMap<String, SessionRecord>>>,
    api_token: Option<String>,
    webhook_secret: Option<String>,
    telegram_secret: Option<String>,
    telegram_bot_token: Option<String>,
    telegram_api_base: String,
    slack_signing_secret: Option<String>,
    slack_bot_token: Option<String>,
    slack_api_base: String,
    discord_public_key: Option<String>,
    discord_bot_token: Option<String>,
    discord_api_base: String,
    persist_enabled: bool,
    persist_path: Arc<PathBuf>,
    rate_limiter: Option<Arc<GlobalRateLimiter>>,
    session_ttl: Option<Duration>,
    metrics: Arc<Metrics>,
    metrics_require_auth: bool,
}

/// 健康检查响应
#[derive(Debug, Serialize, Deserialize)]
struct HealthResponse {
    status: String,
    agent_name: String,
    version: String,
}

/// Webhook 入站请求
#[derive(Debug, Serialize, Deserialize)]
struct InboundWebhookRequest {
    #[serde(default = "default_channel")]
    channel: String,
    chat_id: String,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
}

fn default_channel() -> String {
    "webhook".to_string()
}

/// Telegram Bot API `Update` 的最小子集（手写 serde，不引入 Bot SDK）。
#[derive(Debug, Deserialize)]
struct TelegramUpdate {
    #[serde(default)]
    message: Option<TelegramMessage>,
    #[serde(default)]
    edited_message: Option<TelegramMessage>,
}

/// Telegram `Message` 最小子集：只要 `chat.id` 与可选 `text`。
#[derive(Debug, Deserialize)]
struct TelegramMessage {
    chat: TelegramChat,
    #[serde(default)]
    text: Option<String>,
}

/// Telegram `Chat` 最小子集。
#[derive(Debug, Deserialize)]
struct TelegramChat {
    id: TelegramChatId,
}

/// Bot API 的 `chat.id` 一般为整数，测试/代理也可能给字符串。
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TelegramChatId {
    Int(i64),
    Str(String),
}

impl std::fmt::Display for TelegramChatId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Int(id) => write!(f, "{id}"),
            Self::Str(s) => write!(f, "{s}"),
        }
    }
}

/// 从 Update 取出可入站的 `(chat.id, text)`；无文本则 `None`。
fn telegram_inbound_text(update: &TelegramUpdate) -> Option<(String, String)> {
    let msg = update.message.as_ref().or(update.edited_message.as_ref())?;
    let text = msg
        .text
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    Some((msg.chat.id.to_string(), text.to_string()))
}

/// 按 Telegram 4096 字符上限截断 `sendMessage` 文本。
fn truncate_telegram_text(text: &str) -> &str {
    match text.char_indices().nth(TELEGRAM_MAX_TEXT_LEN) {
        Some((idx, _)) => &text[..idx],
        None => text,
    }
}

/// 错误信息里可能含 Bot Token（minreq 会把 URL 写进 Display），打码后再记日志/回传。
fn redact_secret(text: &str, secret: &str) -> String {
    if secret.is_empty() {
        text.to_string()
    } else {
        text.replace(secret, "***")
    }
}

/// Telegram `sendMessage` 出站结果。失败不得改变 webhook HTTP 状态。
struct TelegramDelivery {
    delivered: bool,
    error: Option<String>,
}

fn send_telegram_message_sync(
    api_base: &str,
    token: &str,
    chat_id: &str,
    text: &str,
) -> Result<(), String> {
    let url = format!("{api_base}/bot{token}/sendMessage");
    let payload = json!({
        "chat_id": chat_id,
        "text": text,
    });
    let response = minreq::post(&url)
        .with_header("Content-Type", "application/json")
        .with_timeout(TELEGRAM_SEND_TIMEOUT_SECS)
        .with_body(payload.to_string())
        .send()
        .map_err(|e| redact_secret(&e.to_string(), token))?;

    let status = response.status_code;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }

    let body = response.as_str().unwrap_or_default();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        if value.get("ok").and_then(serde_json::Value::as_bool) == Some(false) {
            let description = value
                .get("description")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("ok=false");
            return Err(redact_secret(description, token));
        }
    }
    Ok(())
}

async fn send_telegram_reply(
    api_base: &str,
    token: &str,
    chat_id: &str,
    text: &str,
    request_id: &str,
) -> TelegramDelivery {
    let api_base = api_base.trim_end_matches('/').to_string();
    let token = token.to_string();
    let chat_id = chat_id.to_string();
    let text = truncate_telegram_text(text).to_string();
    let request_id = request_id.to_string();

    let result = tokio::task::spawn_blocking(move || {
        send_telegram_message_sync(&api_base, &token, &chat_id, &text)
    })
    .await;

    match result {
        Ok(Ok(())) => TelegramDelivery {
            delivered: true,
            error: None,
        },
        Ok(Err(err)) => {
            tracing::warn!(
                request_id = %request_id,
                error = %err,
                "Telegram sendMessage 失败；仍返回同步 reply，避免 webhook 重试"
            );
            TelegramDelivery {
                delivered: false,
                error: Some(err),
            }
        }
        Err(join_err) => {
            let err = format!("task join failed: {join_err}");
            tracing::warn!(
                request_id = %request_id,
                error = %err,
                "Telegram sendMessage 失败；仍返回同步 reply，避免 webhook 重试"
            );
            TelegramDelivery {
                delivered: false,
                error: Some(err),
            }
        }
    }
}

fn telegram_token_config_source() -> &'static str {
    match std::env::var("JIACLAW_TELEGRAM_BOT_TOKEN") {
        Ok(value) if !value.trim().is_empty() => "环境变量 JIACLAW_TELEGRAM_BOT_TOKEN",
        _ => "配置文件",
    }
}

fn slack_signing_secret_config_source() -> &'static str {
    match std::env::var("JIACLAW_SLACK_SIGNING_SECRET") {
        Ok(value) if !value.trim().is_empty() => "环境变量 JIACLAW_SLACK_SIGNING_SECRET",
        _ => "配置文件",
    }
}

fn slack_token_config_source() -> &'static str {
    match std::env::var("JIACLAW_SLACK_BOT_TOKEN") {
        Ok(value) if !value.trim().is_empty() => "环境变量 JIACLAW_SLACK_BOT_TOKEN",
        _ => "配置文件",
    }
}

fn discord_public_key_config_source() -> &'static str {
    match std::env::var("JIACLAW_DISCORD_PUBLIC_KEY") {
        Ok(value) if !value.trim().is_empty() => "环境变量 JIACLAW_DISCORD_PUBLIC_KEY",
        _ => "配置文件",
    }
}

fn discord_token_config_source() -> &'static str {
    match std::env::var("JIACLAW_DISCORD_BOT_TOKEN") {
        Ok(value) if !value.trim().is_empty() => "环境变量 JIACLAW_DISCORD_BOT_TOKEN",
        _ => "配置文件",
    }
}

/// Slack Events API envelope 最小子集（手写 serde，不引入 Slack SDK）。
#[derive(Debug, Deserialize)]
struct SlackEnvelope {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    challenge: Option<String>,
    #[serde(default)]
    team_id: Option<String>,
    #[serde(default)]
    event: Option<SlackEvent>,
}

/// Slack `event` 最小子集：message 文本入站。
#[derive(Debug, Deserialize)]
struct SlackEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    subtype: Option<String>,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

/// Slack 入站分类。
#[derive(Debug, PartialEq, Eq)]
enum SlackInboundKind {
    UrlVerification {
        challenge: String,
    },
    Message {
        session_id: String,
        channel: String,
        text: String,
    },
    Skipped {
        reason: String,
    },
}

fn optional_nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

fn slack_session_id(team_id: Option<&str>, channel: &str) -> String {
    match optional_nonempty(team_id) {
        Some(team) => format!("slack:{team}:{channel}"),
        None => format!("slack:{channel}"),
    }
}

fn classify_slack_envelope(envelope: &SlackEnvelope) -> SlackInboundKind {
    if envelope.event_type == "url_verification" {
        return SlackInboundKind::UrlVerification {
            challenge: envelope.challenge.clone().unwrap_or_default(),
        };
    }
    if envelope.event_type != "event_callback" {
        return SlackInboundKind::Skipped {
            reason: "ignored event type".to_string(),
        };
    }
    let Some(event) = envelope.event.as_ref() else {
        return SlackInboundKind::Skipped {
            reason: "ignored event type".to_string(),
        };
    };
    if event.event_type != "message" {
        return SlackInboundKind::Skipped {
            reason: "ignored event type".to_string(),
        };
    }
    if optional_nonempty(event.subtype.as_deref()).is_some() {
        return SlackInboundKind::Skipped {
            reason: "ignored message subtype".to_string(),
        };
    }
    let Some(channel) = optional_nonempty(event.channel.as_deref()) else {
        return SlackInboundKind::Skipped {
            reason: "missing channel".to_string(),
        };
    };
    let Some(text) = optional_nonempty(event.text.as_deref()) else {
        return SlackInboundKind::Skipped {
            reason: "no text in event".to_string(),
        };
    };
    SlackInboundKind::Message {
        session_id: slack_session_id(envelope.team_id.as_deref(), channel),
        channel: channel.to_string(),
        text: text.to_string(),
    }
}

/// Discord snowflake：API 多为字符串，测试/代理也可能给数字。
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum DiscordSnowflake {
    String(String),
    Number(u64),
}

impl DiscordSnowflake {
    fn as_trimmed(&self) -> Option<String> {
        match self {
            Self::String(s) => optional_nonempty(Some(s)).map(ToString::to_string),
            Self::Number(n) => Some(n.to_string()),
        }
    }
}

/// Discord Interactions 入站最小子集（手写 serde，不引入 serenity）。
#[derive(Debug, Deserialize)]
struct DiscordInteraction {
    #[serde(rename = "type")]
    interaction_type: u64,
    #[serde(default)]
    application_id: Option<DiscordSnowflake>,
    #[serde(default)]
    guild_id: Option<DiscordSnowflake>,
    #[serde(default)]
    channel_id: Option<DiscordSnowflake>,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    data: Option<DiscordInteractionData>,
}

/// Discord `data` 最小子集：Chat Input Command 名称与选项。
#[derive(Debug, Deserialize)]
struct DiscordInteractionData {
    #[serde(default)]
    name: Option<String>,
    #[serde(rename = "type")]
    #[serde(default)]
    command_type: Option<u64>,
    #[serde(default)]
    options: Vec<DiscordCommandOption>,
}

/// Discord 命令选项（可嵌套 subcommand）。
#[derive(Debug, Deserialize)]
struct DiscordCommandOption {
    #[serde(default)]
    name: String,
    #[serde(rename = "type")]
    #[serde(default)]
    option_type: Option<u64>,
    #[serde(default)]
    value: Option<serde_json::Value>,
    #[serde(default)]
    options: Vec<DiscordCommandOption>,
}

/// Discord 入站分类。
#[derive(Debug, PartialEq, Eq)]
enum DiscordInboundKind {
    Ping,
    ChatCommand {
        session_id: String,
        text: String,
        application_id: String,
        interaction_token: String,
    },
    Skipped {
        reason: String,
    },
}

fn discord_session_id(guild_id: Option<&str>, channel_id: &str) -> String {
    match optional_nonempty(guild_id) {
        Some(guild) => format!("discord:{guild}:{channel_id}"),
        None => format!("discord:dm:{channel_id}"),
    }
}

fn json_value_nonempty_string(value: Option<&serde_json::Value>) -> Option<String> {
    value.and_then(|v| match v {
        serde_json::Value::String(s) => optional_nonempty(Some(s)).map(ToString::to_string),
        _ => None,
    })
}

fn json_value_display(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => optional_nonempty(Some(s)).map(ToString::to_string),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        other => Some(other.to_string()),
    }
}

fn first_string_option(options: &[DiscordCommandOption]) -> Option<String> {
    for option in options {
        if option.option_type.unwrap_or(0) == DISCORD_OPTION_STRING || option.option_type.is_none()
        {
            if let Some(text) = json_value_nonempty_string(option.value.as_ref()) {
                return Some(text);
            }
        }
        if let Some(text) = first_string_option(&option.options) {
            return Some(text);
        }
    }
    None
}

fn flatten_option_parts(options: &[DiscordCommandOption]) -> Vec<String> {
    let mut parts = Vec::new();
    for option in options {
        if option.options.is_empty() {
            match option.value.as_ref().and_then(json_value_display) {
                Some(value) => {
                    if optional_nonempty(Some(&option.name)).is_some() {
                        parts.push(format!("{}={value}", option.name.trim()));
                    } else {
                        parts.push(value);
                    }
                }
                None => {
                    if let Some(name) = optional_nonempty(Some(&option.name)) {
                        parts.push(name.to_string());
                    }
                }
            }
        } else {
            if let Some(name) = optional_nonempty(Some(&option.name)) {
                parts.push(name.to_string());
            }
            parts.extend(flatten_option_parts(&option.options));
        }
    }
    parts
}

fn discord_command_text(data: &DiscordInteractionData) -> Option<String> {
    if let Some(text) = first_string_option(&data.options) {
        return Some(text);
    }
    let mut parts = Vec::new();
    if let Some(name) = optional_nonempty(data.name.as_deref()) {
        parts.push(name.to_string());
    }
    parts.extend(flatten_option_parts(&data.options));
    let text = parts.join(" ");
    optional_nonempty(Some(&text)).map(ToString::to_string)
}

fn classify_discord_interaction(interaction: &DiscordInteraction) -> DiscordInboundKind {
    if interaction.interaction_type == DISCORD_INTERACTION_PING {
        return DiscordInboundKind::Ping;
    }
    if interaction.interaction_type != DISCORD_INTERACTION_APPLICATION_COMMAND {
        return DiscordInboundKind::Skipped {
            reason: "ignored interaction type".to_string(),
        };
    }
    let Some(data) = interaction.data.as_ref() else {
        return DiscordInboundKind::Skipped {
            reason: "missing command data".to_string(),
        };
    };
    if data
        .command_type
        .is_some_and(|command_type| command_type != DISCORD_COMMAND_CHAT_INPUT)
    {
        return DiscordInboundKind::Skipped {
            reason: "ignored command type".to_string(),
        };
    }
    let Some(channel_id) = interaction
        .channel_id
        .as_ref()
        .and_then(DiscordSnowflake::as_trimmed)
    else {
        return DiscordInboundKind::Skipped {
            reason: "missing channel".to_string(),
        };
    };
    let Some(text) = discord_command_text(data) else {
        return DiscordInboundKind::Skipped {
            reason: "no text in command".to_string(),
        };
    };
    let application_id = interaction
        .application_id
        .as_ref()
        .and_then(DiscordSnowflake::as_trimmed)
        .unwrap_or_default();
    let interaction_token = interaction.token.clone().unwrap_or_default();
    let guild_id = interaction
        .guild_id
        .as_ref()
        .and_then(DiscordSnowflake::as_trimmed);
    DiscordInboundKind::ChatCommand {
        session_id: discord_session_id(guild_id.as_deref(), &channel_id),
        text,
        application_id,
        interaction_token,
    }
}

fn verify_discord_signature(
    public_key_hex: &str,
    timestamp: &str,
    body: &[u8],
    signature_hex: &str,
) -> bool {
    let Some(public_key) = decode_hex(public_key_hex) else {
        return false;
    };
    if public_key.len() != 32 {
        return false;
    }
    let Some(signature) = decode_hex(signature_hex) else {
        return false;
    };
    if signature.len() != 64 {
        return false;
    }
    let mut message = Vec::with_capacity(timestamp.len() + body.len());
    message.extend_from_slice(timestamp.as_bytes());
    message.extend_from_slice(body);
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public_key)
        .verify(&message, &signature)
        .is_ok()
}

fn verify_discord_request(
    public_key_hex: &str,
    timestamp: Option<&str>,
    signature: Option<&str>,
    body: &[u8],
) -> bool {
    let Some(timestamp) = timestamp.filter(|s| !s.is_empty()) else {
        return false;
    };
    let Some(signature) = signature.filter(|s| !s.is_empty()) else {
        return false;
    };
    verify_discord_signature(public_key_hex, timestamp, body, signature)
}

fn truncate_discord_text(text: &str) -> &str {
    match text.char_indices().nth(DISCORD_MAX_TEXT_LEN) {
        Some((idx, _)) => &text[..idx],
        None => text,
    }
}

fn discord_followup_content(text: &str) -> String {
    let truncated = truncate_discord_text(text);
    if truncated.trim().is_empty() {
        "(empty reply)".to_string()
    } else {
        truncated.to_string()
    }
}

fn discord_pong_response() -> Response {
    (
        StatusCode::OK,
        Json(json!({ "type": DISCORD_CALLBACK_PONG })),
    )
        .into_response()
}

fn discord_deferred_response() -> Response {
    (
        StatusCode::OK,
        Json(json!({ "type": DISCORD_CALLBACK_DEFERRED_CHANNEL_MESSAGE })),
    )
        .into_response()
}

fn discord_skipped_response(reason: &str) -> Response {
    (
        StatusCode::OK,
        Json(json!({
            "type": DISCORD_CALLBACK_CHANNEL_MESSAGE,
            "data": {
                "content": format!("skipped: {reason}"),
                "flags": DISCORD_FLAG_EPHEMERAL,
            }
        })),
    )
        .into_response()
}

/// Discord deferred follow-up：失败只记日志，不影响已返回的 `type=5` ACK。
fn edit_discord_original_sync(
    api_base: &str,
    token: &str,
    application_id: &str,
    interaction_token: &str,
    text: &str,
) -> Result<(), String> {
    let url =
        format!("{api_base}/webhooks/{application_id}/{interaction_token}/messages/@original");
    let payload = json!({
        "content": text,
    });
    let response = minreq::patch(&url)
        .with_header("Content-Type", "application/json")
        .with_header("Authorization", format!("Bot {token}"))
        .with_timeout(DISCORD_SEND_TIMEOUT_SECS)
        .with_body(payload.to_string())
        .send()
        .map_err(|e| redact_secret(&e.to_string(), token))?;

    let status = response.status_code;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }

    let body = response.as_str().unwrap_or_default();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(message) = value.get("message").and_then(serde_json::Value::as_str) {
            if value.get("code").is_some() {
                return Err(redact_secret(message, token));
            }
        }
    }
    Ok(())
}

async fn edit_discord_original_reply(
    api_base: &str,
    token: &str,
    application_id: &str,
    interaction_token: &str,
    text: &str,
    request_id: &str,
) {
    let api_base = api_base.trim_end_matches('/').to_string();
    let token = token.to_string();
    let application_id = application_id.to_string();
    let interaction_token = interaction_token.to_string();
    let text = discord_followup_content(text);
    let request_id = request_id.to_string();

    let result = tokio::task::spawn_blocking(move || {
        edit_discord_original_sync(
            &api_base,
            &token,
            &application_id,
            &interaction_token,
            &text,
        )
    })
    .await;

    match result {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            tracing::warn!(
                request_id = %request_id,
                error = %err,
                "Discord 编辑原始 Interaction 失败；入站已 ACK deferred"
            );
        }
        Err(join_err) => {
            tracing::warn!(
                request_id = %request_id,
                error = %join_err,
                "Discord 编辑原始 Interaction 失败；入站已 ACK deferred"
            );
        }
    }
}

fn spawn_discord_deferred_chat(
    state: AppState,
    session_id: String,
    text: String,
    application_id: String,
    interaction_token: String,
    request_id: String,
) {
    tokio::spawn(async move {
        let reply =
            match run_session_user_chat(&state, &session_id, &text, &request_id, "discord").await {
                Ok(reply) => reply,
                Err(err) => {
                    tracing::warn!(
                        request_id = %request_id,
                        error = ?err,
                        "Discord deferred chat 失败"
                    );
                    "JiaClaw 处理失败".to_string()
                }
            };

        let Some(token) = state.discord_bot_token.as_deref() else {
            tracing::warn!(
                request_id = %request_id,
                session_id = %session_id,
                "未配置 Discord Bot Token，无法编辑 deferred 回复；session 已记录"
            );
            return;
        };
        if application_id.is_empty() || interaction_token.is_empty() {
            tracing::warn!(
                request_id = %request_id,
                session_id = %session_id,
                "缺少 application_id 或 interaction token，无法编辑 deferred 回复"
            );
            return;
        }
        edit_discord_original_reply(
            &state.discord_api_base,
            token,
            &application_id,
            &interaction_token,
            &reply,
            &request_id,
        )
        .await;
    });
}

#[cfg(test)]
fn encode_hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(char::from(HEX[(byte >> 4) as usize]));
        out.push(char::from(HEX[(byte & 0x0f) as usize]));
    }
    out
}

fn from_hex_digit(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

fn decode_hex(input: &str) -> Option<Vec<u8>> {
    if input.len() % 2 != 0 {
        return None;
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks_exact(2) {
        let hi = from_hex_digit(chunk[0])?;
        let lo = from_hex_digit(chunk[1])?;
        out.push((hi << 4) | lo);
    }
    Some(out)
}

fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn slack_timestamp_fresh(timestamp: &str, now_secs: u64) -> bool {
    timestamp
        .parse::<u64>()
        .is_ok_and(|ts| now_secs.abs_diff(ts) <= SLACK_MAX_TIMESTAMP_SKEW_SECS)
}

/// 计算 Slack 官方 `v0=` HMAC-SHA256 签名（用于测试与对照已知向量）。
#[cfg(test)]
fn slack_v0_signature(secret: &str, timestamp: &str, body: &[u8]) -> Option<String> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(b"v0:");
    mac.update(timestamp.as_bytes());
    mac.update(b":");
    mac.update(body);
    Some(format!(
        "v0={}",
        encode_hex_lower(&mac.finalize().into_bytes())
    ))
}

fn verify_slack_v0_signature(secret: &str, timestamp: &str, body: &[u8], signature: &str) -> bool {
    let Some(hex_sig) = signature.strip_prefix("v0=") else {
        return false;
    };
    let Some(sig_bytes) = decode_hex(hex_sig) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(b"v0:");
    mac.update(timestamp.as_bytes());
    mac.update(b":");
    mac.update(body);
    mac.verify_slice(&sig_bytes).is_ok()
}

fn verify_slack_request(
    secret: &str,
    timestamp: Option<&str>,
    signature: Option<&str>,
    body: &[u8],
    now_secs: u64,
) -> bool {
    let Some(timestamp) = timestamp.filter(|s| !s.is_empty()) else {
        return false;
    };
    if !slack_timestamp_fresh(timestamp, now_secs) {
        return false;
    }
    let Some(signature) = signature.filter(|s| !s.is_empty()) else {
        return false;
    };
    verify_slack_v0_signature(secret, timestamp, body, signature)
}

fn truncate_slack_text(text: &str) -> &str {
    match text.char_indices().nth(SLACK_MAX_TEXT_LEN) {
        Some((idx, _)) => &text[..idx],
        None => text,
    }
}

/// Slack `chat.postMessage` 出站结果。失败不得改变 webhook HTTP 状态。
struct SlackDelivery {
    delivered: bool,
    error: Option<String>,
}

fn send_slack_message_sync(
    api_base: &str,
    token: &str,
    channel: &str,
    text: &str,
) -> Result<(), String> {
    let url = format!("{api_base}/chat.postMessage");
    let payload = json!({
        "channel": channel,
        "text": text,
    });
    let response = minreq::post(&url)
        .with_header("Content-Type", "application/json")
        .with_header("Authorization", format!("Bearer {token}"))
        .with_timeout(SLACK_SEND_TIMEOUT_SECS)
        .with_body(payload.to_string())
        .send()
        .map_err(|e| redact_secret(&e.to_string(), token))?;

    let status = response.status_code;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }

    let body = response.as_str().unwrap_or_default();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        if value.get("ok").and_then(serde_json::Value::as_bool) == Some(false) {
            let description = value
                .get("error")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("ok=false");
            return Err(redact_secret(description, token));
        }
    }
    Ok(())
}

async fn send_slack_reply(
    api_base: &str,
    token: &str,
    channel: &str,
    text: &str,
    request_id: &str,
) -> SlackDelivery {
    let api_base = api_base.trim_end_matches('/').to_string();
    let token = token.to_string();
    let channel = channel.to_string();
    let text = truncate_slack_text(text).to_string();
    let request_id = request_id.to_string();

    let result = tokio::task::spawn_blocking(move || {
        send_slack_message_sync(&api_base, &token, &channel, &text)
    })
    .await;

    match result {
        Ok(Ok(())) => SlackDelivery {
            delivered: true,
            error: None,
        },
        Ok(Err(err)) => {
            tracing::warn!(
                request_id = %request_id,
                error = %err,
                "Slack chat.postMessage 失败；仍返回同步 reply，避免 Events API 重试"
            );
            SlackDelivery {
                delivered: false,
                error: Some(err),
            }
        }
        Err(join_err) => {
            let err = format!("task join failed: {join_err}");
            tracing::warn!(
                request_id = %request_id,
                error = %err,
                "Slack chat.postMessage 失败；仍返回同步 reply，避免 Events API 重试"
            );
            SlackDelivery {
                delivered: false,
                error: Some(err),
            }
        }
    }
}

/// Slack 入站响应：URL 验证以外，有文本时同步回传 assistant 文本。
#[derive(Debug, Serialize, Deserialize)]
struct SlackInboundResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    skipped: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    delivered: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    delivery_error: Option<String>,
}

/// Slack URL 验证响应：必须是 `{ challenge }`。
#[derive(Debug, Serialize)]
struct SlackChallengeResponse {
    challenge: String,
}

/// Telegram 入站响应：有文本时同步回传 assistant 文本，便于长轮询调试。
#[derive(Debug, Serialize, Deserialize)]
struct TelegramInboundResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    skipped: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    /// 是否已成功调用 Bot `sendMessage`；未配置 token 时省略，保持与仅入站切片兼容。
    #[serde(skip_serializing_if = "Option::is_none")]
    delivered: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    delivery_error: Option<String>,
}

fn build_rate_limiter(per_minute: u32) -> Option<Arc<GlobalRateLimiter>> {
    NonZeroU32::new(per_minute).map(|nz| Arc::new(RateLimiter::direct(Quota::per_minute(nz))))
}

fn is_rate_limited_path(path: &str) -> bool {
    path.starts_with("/api/")
        || path == "/hooks/inbound"
        || path == "/hooks/telegram"
        || path == "/hooks/slack"
        || path == "/hooks/discord"
}

/// 全局限流中间件：返回 `Response`，不依赖 `ConnectInfo`，也不使用 `Err(StatusCode)`。
async fn rate_limit_middleware(
    State(limiter): State<Option<Arc<GlobalRateLimiter>>>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_owned();
    if !is_rate_limited_path(&path) {
        return next.run(request).await;
    }

    let Some(limiter) = limiter.as_ref() else {
        return next.run(request).await;
    };

    match limiter.check() {
        Ok(()) => next.run(request).await,
        Err(not_until) => {
            let wait = not_until.wait_time_from(DefaultClock::default().now());
            let retry_after_secs = wait.as_secs().max(1);
            tracing::warn!(path, retry_after_secs, "HTTP 请求超过速率限制");
            rate_limited_response(retry_after_secs)
        }
    }
}

/// HTTP 请求计数：在 handler / 限流 429 之后按路由族累加。
async fn metrics_middleware(
    State(metrics): State<Arc<Metrics>>,
    request: Request,
    next: Next,
) -> Response {
    let family = classify_http_path(request.uri().path());
    let method = request.method().clone();
    let response = next.run(request).await;
    metrics.record_http(family, method.as_str(), response.status().as_u16());
    response
}

fn rate_limited_response(retry_after_secs: u64) -> Response {
    let mut response = (
        StatusCode::TOO_MANY_REQUESTS,
        Json(json!({"error": "rate_limit_exceeded"})),
    )
        .into_response();
    if let Ok(value) = HeaderValue::from_str(&retry_after_secs.to_string()) {
        response.headers_mut().insert(header::RETRY_AFTER, value);
    }
    response
}

/// 从请求头读取 `X-Request-Id`；缺失或为空则生成 UUID。
fn resolve_request_id(headers: &HeaderMap) -> String {
    headers
        .get(X_REQUEST_ID)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map_or_else(|| uuid::Uuid::new_v4().to_string(), ToOwned::to_owned)
}

/// 供 tracing 日志使用的 `request_id`；中间件保证响应侧必有该头。
fn request_id_log_value(headers: &HeaderMap) -> &str {
    headers
        .get(X_REQUEST_ID)
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .unwrap_or("-")
}

/// 请求追踪中间件：缺省生成 UUID，响应回写 `X-Request-Id`（含 `/health` 与 429）。
async fn request_id_middleware(mut request: Request, next: Next) -> Response {
    let request_id = resolve_request_id(request.headers());
    let Ok(header_value) = HeaderValue::from_str(&request_id) else {
        let mut response = next.run(request).await;
        if let Ok(generated) = HeaderValue::from_str(&uuid::Uuid::new_v4().to_string()) {
            response.headers_mut().insert(X_REQUEST_ID, generated);
        }
        return response;
    };

    request
        .headers_mut()
        .insert(X_REQUEST_ID, header_value.clone());
    let mut response = next.run(request).await;
    response.headers_mut().insert(X_REQUEST_ID, header_value);
    response
}

fn build_router(state: AppState) -> Router {
    let limiter = state.rate_limiter.clone();
    let metrics = state.metrics.clone();
    Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .route("/api/chat", post(chat_handler))
        .route(
            "/api/sessions",
            get(list_sessions_handler).post(create_session_handler),
        )
        .route(
            "/api/sessions/:id",
            get(get_session_handler).delete(delete_session_handler),
        )
        .route("/api/sessions/:id/export", get(export_session_handler))
        .route("/api/tools", get(tools_handler))
        .route("/api/skills", get(skills_handler))
        .route("/api/openapi.json", get(openapi_handler))
        .route("/hooks/inbound", post(hooks_inbound_handler))
        .route("/hooks/telegram", post(hooks_telegram_handler))
        .route("/hooks/slack", post(hooks_slack_handler))
        .route("/hooks/discord", post(hooks_discord_handler))
        .layer(middleware::from_fn_with_state(
            limiter,
            rate_limit_middleware,
        ))
        .layer(middleware::from_fn_with_state(metrics, metrics_middleware))
        // 外层：即使限流 429 也回写 X-Request-Id
        .layer(middleware::from_fn(request_id_middleware))
        .with_state(state)
}

fn rate_limit_config_source() -> &'static str {
    if std::env::var("JIACLAW_RATE_LIMIT_PER_MINUTE").is_ok() {
        "环境变量 JIACLAW_RATE_LIMIT_PER_MINUTE"
    } else {
        "配置文件"
    }
}

fn metrics_auth_config_source() -> &'static str {
    if std::env::var("JIACLAW_METRICS_REQUIRE_AUTH").is_ok() {
        "环境变量 JIACLAW_METRICS_REQUIRE_AUTH"
    } else {
        "配置文件"
    }
}

fn session_ttl_config_source() -> &'static str {
    if std::env::var("JIACLAW_SESSION_TTL_SECS").is_ok() {
        "环境变量 JIACLAW_SESSION_TTL_SECS"
    } else {
        "配置文件"
    }
}

fn tool_timeout_config_source() -> &'static str {
    if std::env::var("JIACLAW_TOOL_TIMEOUT_SECS").is_ok() {
        "环境变量 JIACLAW_TOOL_TIMEOUT_SECS"
    } else {
        "配置文件"
    }
}

fn max_tool_iterations_config_source() -> &'static str {
    if std::env::var("JIACLAW_MAX_TOOL_ITERATIONS").is_ok() {
        "环境变量 JIACLAW_MAX_TOOL_ITERATIONS"
    } else {
        "配置文件"
    }
}

fn max_tool_iterations_status_line(config: &AgentConfig) -> String {
    format!(
        "{}（通过 {}）",
        config.effective_max_tool_iterations(),
        max_tool_iterations_config_source()
    )
}

fn heartbeat_interval_config_source() -> &'static str {
    if std::env::var("JIACLAW_HEARTBEAT_INTERVAL_SECS").is_ok() {
        "环境变量 JIACLAW_HEARTBEAT_INTERVAL_SECS"
    } else {
        "配置文件"
    }
}

fn session_summarize_config_source() -> &'static str {
    if std::env::var("JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW").is_ok() {
        "环境变量 JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW"
    } else {
        "配置文件"
    }
}

fn brave_api_key_config_source() -> &'static str {
    match std::env::var("JIACLAW_BRAVE_API_KEY") {
        Ok(value) if !value.trim().is_empty() => "环境变量 JIACLAW_BRAVE_API_KEY",
        _ => "配置文件",
    }
}

fn web_search_status_lines(config: &AgentConfig) -> Vec<String> {
    if !config.tools.web_search.enabled {
        return vec!["已关闭（[tools.web_search] enabled = false，未注册）".to_string()];
    }
    if config.tools.web_search.effective_brave_api_key().is_some() {
        vec![format!(
            "已启用（Brave API key 已配置，通过 {}，明文不打印）",
            brave_api_key_config_source()
        )]
    } else {
        vec![
            "已启用但未配置 Brave API key（调用将返回友好错误）".to_string(),
            "💡 设置环境变量: export JIACLAW_BRAVE_API_KEY=your-key".to_string(),
            "💡 或配置 [tools.web_search] brave_api_key（不要把 key 提交到仓库）".to_string(),
        ]
    }
}

fn web_fetch_status_line(config: &AgentConfig) -> String {
    if !config.tools.web_fetch.enabled {
        return "已关闭（[tools.web_fetch] enabled = false，未注册）".to_string();
    }
    if config.tools.web_fetch.allow_private {
        "已启用（allow_private = true，允许 localhost/私网）".to_string()
    } else {
        "已启用（默认拒绝 localhost/私网）".to_string()
    }
}

fn memory_search_status_line(config: &AgentConfig) -> String {
    if !config.tools.memory_search.enabled {
        return "已关闭（[tools.memory_search] enabled = false，未注册）".to_string();
    }
    "已启用（默认扫描 MEMORY / SOUL / USER；子串检索，无向量库）".to_string()
}

fn session_summarize_status_line(config: &AgentConfig) -> String {
    if config.session.effective_summarize_on_overflow() {
        format!(
            "已启用（keep_recent={}, 通过 {}）",
            config.session.effective_keep_recent(),
            session_summarize_config_source()
        )
    } else {
        "未启用（超过上限硬截断）".to_string()
    }
}

fn heartbeat_file_status_label(config: &AgentConfig) -> String {
    match inspect_heartbeat_file(&config.workspace_path, &config.heartbeat.path) {
        Ok(status) if status.exists => format!("存在 ({} bytes)", status.size_bytes),
        Ok(_) => "不存在".to_string(),
        Err(e) => format!("路径无效 ({e})"),
    }
}

/// 心跳一轮的结果（供测试断言；serve 循环忽略具体值）。
#[derive(Debug, Clone, PartialEq, Eq)]
enum HeartbeatTickOutcome {
    SkippedEmptyOrMissing,
    Completed {
        user_chars: usize,
        reply_chars: usize,
    },
    Failed(String),
}

type HeartbeatTickHook = Arc<dyn Fn() + Send + Sync>;

fn maybe_spawn_heartbeat(
    state: AppState,
    config: &AgentConfig,
    on_tick: Option<HeartbeatTickHook>,
) -> Option<tokio::task::JoinHandle<()>> {
    if !config.heartbeat.enabled {
        return None;
    }

    let rel_path = config.heartbeat.path.clone();
    if let Err(e) = resolve_heartbeat_path(&config.workspace_path, &rel_path) {
        tracing::warn!("Heartbeat 已启用但路径无效，不启动后台任务: {e}");
        return None;
    }

    let workspace = config.workspace_path.clone();
    let session_id = config.heartbeat.effective_session_id().to_string();
    let interval = Duration::from_secs(config.heartbeat.effective_interval_secs());

    Some(tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        ticker.tick().await;
        loop {
            ticker.tick().await;
            let _ = run_heartbeat_tick(&state, &workspace, &rel_path, &session_id).await;
            if let Some(hook) = &on_tick {
                hook();
            }
        }
    }))
}

async fn run_heartbeat_tick(
    state: &AppState,
    workspace: &std::path::Path,
    rel_path: &str,
    session_id: &str,
) -> HeartbeatTickOutcome {
    let content = match load_heartbeat_message(workspace, rel_path) {
        Ok(Some(text)) => text,
        Ok(None) => {
            tracing::debug!(
                session_id,
                path = rel_path,
                "Heartbeat 跳过本轮：文件缺失或为空"
            );
            return HeartbeatTickOutcome::SkippedEmptyOrMissing;
        }
        Err(e) => {
            tracing::warn!(session_id, "Heartbeat 本轮跳过：无法读取文件: {e}");
            return HeartbeatTickOutcome::Failed(e.to_string());
        }
    };

    let user_chars = content.chars().count();
    match run_session_user_chat(state, session_id, &content, "heartbeat", "heartbeat").await {
        Ok(reply) => {
            let reply_chars = reply.chars().count();
            tracing::info!(session_id, user_chars, reply_chars, "Heartbeat 完成一轮");
            HeartbeatTickOutcome::Completed {
                user_chars,
                reply_chars,
            }
        }
        Err(e) => {
            let message = match e {
                AppError::Internal(msg) => msg,
                AppError::Unauthorized => "Unauthorized".to_string(),
                AppError::NotFound => "NotFound".to_string(),
            };
            tracing::warn!(session_id, "Heartbeat 本轮 chat 失败: {message}");
            HeartbeatTickOutcome::Failed(message)
        }
    }
}

fn sessions_from_messages(
    raw: HashMap<String, Vec<ChatMessage>>,
) -> HashMap<String, SessionRecord> {
    raw.into_iter()
        .map(|(id, messages)| (id, SessionRecord::new(messages)))
        .collect()
}

fn messages_from_sessions(
    sessions: &HashMap<String, SessionRecord>,
) -> HashMap<String, Vec<ChatMessage>> {
    sessions
        .iter()
        .map(|(id, rec)| (id.clone(), rec.messages.clone()))
        .collect()
}

fn persist_session_map(state: &AppState, sessions: &HashMap<String, SessionRecord>) {
    if !state.persist_enabled {
        return;
    }
    let raw = messages_from_sessions(sessions);
    if let Err(e) = save_sessions(&state.persist_path, &raw) {
        tracing::error!("保存 sessions 失败: {}", e);
    }
}

/// 合并 session 历史与本轮入站消息，并在超过上限时压缩（共享写入前的唯一裁剪点）。
///
/// HTTP `/api/chat`、webhook / Telegram / Slack / heartbeat 都走这里，避免通道分叉。
async fn prepare_session_chat_messages(
    state: &AppState,
    session_id: &str,
    incoming: Vec<ChatMessage>,
    request_id: &str,
    channel_label: &str,
) -> Vec<ChatMessage> {
    let history = {
        let sessions = state.sessions.lock().unwrap();
        sessions.get(session_id).map(|rec| rec.messages.clone())
    };

    let Some(history) = history else {
        tracing::info!(
            request_id = %request_id,
            "创建新 {channel_label} session: {session_id}"
        );
        return incoming;
    };

    let mut all_messages = history;
    all_messages.extend(incoming);
    let before = all_messages.len();
    if before > MAX_SESSION_MESSAGES {
        tracing::info!(
            request_id = %request_id,
            "{channel_label} session {session_id} 消息数 {before} 超过上限 {MAX_SESSION_MESSAGES}，开始压缩"
        );
    }

    let compacted = state.agent.compact_session_messages(all_messages).await;
    tracing::info!(
        request_id = %request_id,
        "使用 {channel_label} session {session_id}, 合并后消息数: {} (压缩前 {before})",
        compacted.len()
    );
    compacted
}

/// 将压缩后的完整历史写回共享 session store（含可选落盘）。
fn commit_session_messages(
    state: &AppState,
    session_id: &str,
    messages: Vec<ChatMessage>,
    request_id: &str,
    channel_label: &str,
) {
    let mut sessions = state.sessions.lock().unwrap();
    let message_count = messages.len();
    sessions.insert(session_id.to_string(), SessionRecord::new(messages));
    tracing::info!(
        request_id = %request_id,
        "更新 {channel_label} session {session_id}, 当前消息数: {message_count}"
    );
    persist_session_map(state, &sessions);
}

fn purge_expired_sessions(state: &AppState) -> usize {
    let Some(ttl) = state.session_ttl else {
        return 0;
    };
    let now = Instant::now();
    let mut sessions = state.sessions.lock().unwrap();
    let before = sessions.len();
    sessions.retain(|id, rec| {
        let keep = !rec.is_expired(ttl, now);
        if !keep {
            tracing::info!("Session {id} 已闲置过期，移出 store");
        }
        keep
    });
    let removed = before.saturating_sub(sessions.len());
    if removed > 0 {
        persist_session_map(state, &sessions);
    }
    removed
}

fn spawn_session_ttl_sweeper(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(SESSION_TTL_SWEEP_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        loop {
            interval.tick().await;
            let removed = purge_expired_sessions(&state);
            if removed > 0 {
                tracing::info!("Session TTL 扫描移除 {removed} 个过期会话");
            }
        }
    });
}

/// Webhook 入站响应
#[derive(Debug, Serialize, Deserialize)]
struct InboundWebhookResponse {
    ok: bool,
    session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[allow(clippy::too_many_lines)]
async fn serve_command(config_path: Option<PathBuf>, bind: Option<String>) -> Result<()> {
    // 加载配置
    let mut config = if let Some(path) = config_path {
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

    // 命令行参数覆盖配置文件（如果提供）
    if let Some(bind_addr) = bind {
        config.http.bind = bind_addr;
    }

    tracing::info!("正在启动 JiaClaw Agent 服务");
    tracing::info!("使用 Agent 配置: {}", config.name);

    // 创建 agent 与进程内指标（工具钩子在 Arc 包装前挂上）
    let metrics = Arc::new(Metrics::default());
    let agent = attach_tool_metrics(
        JiaClawAgent::new(config.clone()).context("创建 JiaClawAgent 失败")?,
        &metrics,
    );

    // 读取 API token（环境变量优先于配置文件）
    let api_token = std::env::var("JIACLAW_API_TOKEN")
        .ok()
        .or(config.http.api_token.clone());

    // 读取 webhook secret（环境变量优先于配置文件）
    let webhook_secret = std::env::var("JIACLAW_WEBHOOK_SECRET")
        .ok()
        .or(config.http.webhook_secret.clone());

    // 读取 Telegram secret token（环境变量优先于配置文件）
    let telegram_secret = std::env::var("JIACLAW_TELEGRAM_SECRET")
        .ok()
        .or(config.http.telegram_secret.clone());

    // 读取 Telegram Bot API token（环境变量优先于配置文件）
    let telegram_bot_token = config.http.effective_telegram_bot_token();

    // 读取 Slack signing secret / Bot token（环境变量优先于配置文件）
    let slack_signing_secret = config.http.effective_slack_signing_secret();
    let slack_bot_token = config.http.effective_slack_bot_token();
    let discord_public_key = config.http.effective_discord_public_key();
    let discord_bot_token = config.http.effective_discord_bot_token();

    // 读取限流配置（环境变量优先于配置文件）
    let rate_limit_per_minute = config.http.effective_rate_limit_per_minute();
    let rate_limiter = rate_limit_per_minute.and_then(build_rate_limiter);

    // 读取会话闲置 TTL（环境变量优先于配置文件）
    let session_ttl_secs = config.http.effective_session_ttl_secs();
    let session_ttl = session_ttl_secs.map(Duration::from_secs);
    let metrics_public = config.http.effective_metrics_public();
    let metrics_require_auth = !metrics_public;

    // 解析持久化路径
    let persist_path = persist_path_from_config(&config);

    // 加载持久化的 sessions（如果启用）
    let sessions = if config.http.persist {
        tracing::info!("Session 持久化已启用，路径: {}", persist_path.display());
        sessions_from_messages(load_sessions(&persist_path))
    } else {
        tracing::info!("Session 持久化未启用");
        HashMap::new()
    };

    let state = AppState {
        agent: Arc::new(agent),
        sessions: Arc::new(Mutex::new(sessions)),
        api_token: api_token.clone(),
        webhook_secret: webhook_secret.clone(),
        telegram_secret: telegram_secret.clone(),
        telegram_bot_token: telegram_bot_token.clone(),
        telegram_api_base: TELEGRAM_API_BASE.to_string(),
        slack_signing_secret: slack_signing_secret.clone(),
        slack_bot_token: slack_bot_token.clone(),
        slack_api_base: SLACK_API_BASE.to_string(),
        discord_public_key: discord_public_key.clone(),
        discord_bot_token: discord_bot_token.clone(),
        discord_api_base: DISCORD_API_BASE.to_string(),
        persist_enabled: config.http.persist,
        persist_path: Arc::new(persist_path),
        rate_limiter,
        session_ttl,
        metrics,
        metrics_require_auth,
    };

    if state.session_ttl.is_some() {
        spawn_session_ttl_sweeper(state.clone());
    }

    let heartbeat_interval_secs = config.heartbeat.effective_interval_secs();
    if maybe_spawn_heartbeat(state.clone(), &config, None).is_some() {
        tracing::info!(
            interval_secs = heartbeat_interval_secs,
            session_id = config.heartbeat.effective_session_id(),
            path = %config.heartbeat.path,
            file = %heartbeat_file_status_label(&config),
            "Heartbeat 已启用"
        );
    } else if config.heartbeat.enabled {
        tracing::warn!("Heartbeat 配置为启用，但未能启动后台任务");
    } else {
        tracing::info!("Heartbeat 未启用");
    }

    // 配置 CORS
    let cors = if config.http.cors_allow_origins.is_empty()
        || (config.http.cors_allow_origins.len() == 1 && config.http.cors_allow_origins[0] == "*")
    {
        // 允许所有来源
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([Method::GET, Method::POST, Method::DELETE])
            .allow_headers(Any)
    } else {
        // 限制特定来源
        let origins: Vec<_> = config
            .http
            .cors_allow_origins
            .iter()
            .filter_map(|s| s.parse().ok())
            .collect();
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([Method::GET, Method::POST, Method::DELETE])
            .allow_headers(Any)
    };

    // 构建路由（限流中间件不依赖 ConnectInfo，oneshot 测试不会 500）
    let app = build_router(state).layer(cors);

    // 绑定地址
    let listener = tokio::net::TcpListener::bind(&config.http.bind)
        .await
        .with_context(|| format!("无法绑定到地址: {}", config.http.bind))?;

    // 启动日志
    tracing::info!("✅ HTTP 服务已启动于 http://{}", config.http.bind);
    tracing::info!("   • GET    /health              - 健康检查");
    tracing::info!("   • GET    /metrics             - Prometheus 文本指标");
    tracing::info!("   • POST   /api/chat            - 聊天端点（可选 SSE）");
    tracing::info!("   • GET    /api/sessions        - 列出会话");
    tracing::info!("   • POST   /api/sessions        - 创建会话");
    tracing::info!("   • GET    /api/sessions/:id    - 读取会话历史");
    tracing::info!("   • GET    /api/sessions/:id/export - 导出会话（JSONL / JSON）");
    tracing::info!("   • DELETE /api/sessions/:id    - 删除会话");
    tracing::info!("   • GET    /api/tools           - 列出已注册工具");
    tracing::info!("   • GET    /api/skills          - 列出已发现技能");
    tracing::info!("   • GET    /api/openapi.json    - OpenAPI 3 草图");
    tracing::info!("   • POST   /hooks/inbound       - Webhook 入站端点");
    tracing::info!("   • POST   /hooks/telegram      - Telegram Bot 入站端点");
    tracing::info!("   • POST   /hooks/slack         - Slack Events API 入站端点");
    tracing::info!("   • POST   /hooks/discord       - Discord Interactions 入站端点");
    if telegram_bot_token.is_some() {
        tracing::info!("   • Telegram 出站: 已配置 Bot Token（成功回复后调用 sendMessage）");
    } else {
        tracing::info!("   • Telegram 出站: 未配置 Bot Token（仅同步 JSON reply）");
    }
    if slack_bot_token.is_some() {
        tracing::info!("   • Slack 出站: 已配置 Bot Token（成功回复后调用 chat.postMessage）");
    } else {
        tracing::info!("   • Slack 出站: 未配置 Bot Token（仅同步 JSON reply）");
    }
    if discord_bot_token.is_some() {
        tracing::info!(
            "   • Discord 出站: 已配置 Bot Token（deferred 后 PATCH 编辑原始 Interaction）"
        );
    } else {
        tracing::info!("   • Discord 出站: 未配置 Bot Token（deferred ACK 后仅记 session）");
    }
    tracing::info!("   • X-Request-Id                - 请求无该头则生成 UUID 并回写");
    if let Some(limit) = rate_limit_per_minute {
        tracing::info!(
            "   • HTTP 限流: {limit} 次/分钟（/api/* 与 /hooks/inbound、/hooks/telegram、/hooks/slack、/hooks/discord；GET /health 与 GET /metrics 不限流）"
        );
    } else {
        tracing::info!("   • HTTP 限流: 未启用");
    }
    if metrics_public {
        tracing::info!("   • Metrics: 公开（GET /metrics 无需 API Bearer，便于 scrape）");
    } else {
        tracing::info!(
            "   • Metrics: 需 API 鉴权（与 /api/* 相同，通过 {}）",
            metrics_auth_config_source()
        );
    }
    if let Some(ttl) = session_ttl_secs {
        tracing::info!("   • Session TTL: 已启用（闲置 {ttl} 秒后过期）");
    } else {
        tracing::info!("   • Session TTL: 未启用");
    }
    tracing::info!(
        "   • Session 摘要压缩: {}",
        session_summarize_status_line(&config)
    );
    if let Some(secs) = config.effective_tool_timeout_secs() {
        tracing::info!("   • 工具超时: 已启用（每调用 {secs} 秒）");
    } else {
        tracing::info!("   • 工具超时: 未启用（不限制）");
    }
    tracing::info!(
        "   • 工具循环上限: {}",
        max_tool_iterations_status_line(&config)
    );
    if config.tools.web_search.enabled {
        if config.tools.web_search.effective_brave_api_key().is_some() {
            tracing::info!("   • web_search: 已启用（Brave API key 已配置，明文不打印）");
        } else {
            tracing::info!("   • web_search: 已启用但未配置 Brave API key（调用将返回友好错误）");
        }
    } else {
        tracing::info!("   • web_search: 已关闭（未注册）");
    }
    if config.tools.web_fetch.enabled {
        if config.tools.web_fetch.allow_private {
            tracing::info!("   • web_fetch: 已启用（allow_private = true，允许 localhost/私网）");
        } else {
            tracing::info!("   • web_fetch: 已启用（默认拒绝 localhost/私网）");
        }
    } else {
        tracing::info!("   • web_fetch: 已关闭（未注册）");
    }
    if config.tools.memory_search.enabled {
        tracing::info!("   • memory_search: 已启用（默认扫描 MEMORY / SOUL / USER）");
    } else {
        tracing::info!("   • memory_search: 已关闭（未注册）");
    }
    if config.heartbeat.enabled {
        tracing::info!(
            "   • Heartbeat: 已启用（间隔 {heartbeat_interval_secs} 秒，session={}, 文件 {}）",
            config.heartbeat.effective_session_id(),
            heartbeat_file_status_label(&config)
        );
    } else {
        tracing::info!("   • Heartbeat: 未启用");
    }

    // 打印配置摘要（不打印 secret 明文）
    println!("\n📋 HTTP 配置摘要:");
    println!("   • 绑定地址: {}", config.http.bind);

    if api_token.is_some() {
        println!(
            "   • API 鉴权: ✅ 已启用（通过 {}）",
            if std::env::var("JIACLAW_API_TOKEN").is_ok() {
                "环境变量 JIACLAW_API_TOKEN"
            } else {
                "配置文件"
            }
        );
    } else {
        println!("   • API 鉴权: ⚠️  未启用（API 端点无需鉴权，本地开发友好）");
    }

    if webhook_secret.is_some() {
        println!(
            "   • Webhook 鉴权: ✅ 已启用（通过 {}）",
            if std::env::var("JIACLAW_WEBHOOK_SECRET").is_ok() {
                "环境变量 JIACLAW_WEBHOOK_SECRET"
            } else {
                "配置文件"
            }
        );
    } else {
        println!("   • Webhook 鉴权: ⚠️  未启用（任何请求都可访问 /hooks/inbound）");
    }

    if telegram_secret.is_some() {
        println!(
            "   • Telegram 鉴权: ✅ 已启用（通过 {}）",
            if std::env::var("JIACLAW_TELEGRAM_SECRET").is_ok() {
                "环境变量 JIACLAW_TELEGRAM_SECRET"
            } else {
                "配置文件"
            }
        );
    } else {
        println!("   • Telegram 鉴权: ⚠️  未启用（任何请求都可访问 /hooks/telegram）");
    }

    if telegram_bot_token.is_some() {
        println!(
            "   • Telegram Bot Token: ✅ 已配置（通过 {}，明文不打印；将 sendMessage 出站）",
            telegram_token_config_source()
        );
    } else {
        println!("   • Telegram Bot Token: ⚠️  未配置（仅同步 JSON reply，不调用 sendMessage）");
    }

    if slack_signing_secret.is_some() {
        println!(
            "   • Slack 签名校验: ✅ 已启用（通过 {}）",
            slack_signing_secret_config_source()
        );
    } else {
        println!("   • Slack 签名校验: ⚠️  未启用（任何请求都可访问 /hooks/slack）");
    }

    if slack_bot_token.is_some() {
        println!(
            "   • Slack Bot Token: ✅ 已配置（通过 {}，明文不打印；将 chat.postMessage 出站）",
            slack_token_config_source()
        );
    } else {
        println!("   • Slack Bot Token: ⚠️  未配置（仅同步 JSON reply，不调用 chat.postMessage）");
    }

    if discord_public_key.is_some() {
        println!(
            "   • Discord 签名校验: ✅ 已启用（通过 {}）",
            discord_public_key_config_source()
        );
    } else {
        println!("   • Discord 签名校验: ⚠️  未启用（任何请求都可访问 /hooks/discord）");
    }

    if discord_bot_token.is_some() {
        println!(
            "   • Discord Bot Token: ✅ 已配置（通过 {}，明文不打印；将 PATCH 编辑 deferred 回复）",
            discord_token_config_source()
        );
    } else {
        println!("   • Discord Bot Token: ⚠️  未配置（deferred ACK 后仅记 session，不 follow-up）");
    }

    if let Some(limit) = rate_limit_per_minute {
        println!(
            "   • HTTP 限流: ✅ 已启用（{limit} 次/分钟，通过 {}）",
            rate_limit_config_source()
        );
    } else {
        println!(
            "   • HTTP 限流: ⚠️  未启用（/api/* 与 /hooks/inbound、/hooks/telegram、/hooks/slack、/hooks/discord 不限流）"
        );
    }

    if metrics_public {
        println!("   • Metrics: ✅ 公开（GET /metrics 无需鉴权，不计入限流）");
    } else {
        println!(
            "   • Metrics: 🔒 需 API 鉴权（与 /api/* 相同，通过 {}，不计入限流）",
            metrics_auth_config_source()
        );
    }

    if let Some(ttl) = session_ttl_secs {
        println!(
            "   • Session TTL: ✅ 已启用（闲置 {ttl} 秒，通过 {}）",
            session_ttl_config_source()
        );
    } else {
        println!("   • Session TTL: ⚠️  未启用（会话不会因闲置过期）");
    }

    if config.session.effective_summarize_on_overflow() {
        println!(
            "   • Session 摘要压缩: ✅ {}",
            session_summarize_status_line(&config)
        );
    } else {
        println!(
            "   • Session 摘要压缩: ⚠️  {}；可设置 [session] summarize_on_overflow = true 或 JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW=1",
            session_summarize_status_line(&config)
        );
    }

    if let Some(secs) = config.effective_tool_timeout_secs() {
        println!(
            "   • 工具超时: ✅ 已启用（每调用 {secs} 秒，通过 {}）",
            tool_timeout_config_source()
        );
    } else {
        println!("   • 工具超时: ⚠️  未启用（不限制单次工具执行时间）");
    }

    println!(
        "   • 工具循环上限: {}",
        max_tool_iterations_status_line(&config)
    );

    if config.tools.web_search.enabled {
        if config.tools.web_search.effective_brave_api_key().is_some() {
            println!(
                "   • web_search: ✅ 已启用（Brave API key 已配置，通过 {}，明文不打印）",
                brave_api_key_config_source()
            );
        } else {
            println!("   • web_search: ⚠️  已启用但未配置 Brave API key（调用将返回友好错误）");
        }
    } else {
        println!("   • web_search: ⚠️  已关闭（[tools.web_search] enabled = false）");
    }

    if config.tools.web_fetch.enabled {
        if config.tools.web_fetch.allow_private {
            println!("   • web_fetch: ✅ 已启用（allow_private = true，允许 localhost/私网）");
        } else {
            println!("   • web_fetch: ✅ 已启用（默认拒绝 localhost/私网）");
        }
    } else {
        println!("   • web_fetch: ⚠️  已关闭（[tools.web_fetch] enabled = false）");
    }

    if config.tools.memory_search.enabled {
        println!(
            "   • memory_search: ✅ {}",
            memory_search_status_line(&config)
        );
    } else {
        println!(
            "   • memory_search: ⚠️  {}",
            memory_search_status_line(&config)
        );
    }

    if config.heartbeat.enabled {
        println!(
            "   • Heartbeat: ✅ 已启用（间隔 {heartbeat_interval_secs} 秒，session={}, 文件 {}，间隔来自 {}）",
            config.heartbeat.effective_session_id(),
            heartbeat_file_status_label(&config),
            heartbeat_interval_config_source()
        );
    } else {
        println!("   • Heartbeat: ⚠️  未启用（仅 serve 进程可挂后台任务；CLI chat 不跑心跳）");
    }

    if config.http.cors_allow_origins.is_empty()
        || (config.http.cors_allow_origins.len() == 1 && config.http.cors_allow_origins[0] == "*")
    {
        println!("   • CORS 模式: 允许所有来源（Permissive）");
    } else {
        println!(
            "   • CORS 模式: 限制来源（仅允许: {}）",
            config.http.cors_allow_origins.join(", ")
        );
    }

    println!("\n💡 试试：curl http://{}/health", config.http.bind);
    println!("         curl http://{}/metrics", config.http.bind);
    println!("按 Ctrl+C 停止服务\n");

    // 启动服务器
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("服务器运行失败")?;

    tracing::info!("服务器已关闭");
    Ok(())
}

/// 检查 API Token 鉴权
fn check_api_auth(state: &AppState, headers: &HeaderMap) -> bool {
    // 如果未配置 token，则不需要鉴权
    let Some(expected_token) = state.api_token.as_deref() else {
        return true;
    };

    // 检查 Authorization: Bearer <token>
    if let Some(auth_header) = headers.get("Authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                return token == expected_token;
            }
        }
    }

    // 检查 X-Api-Token: <token>
    if let Some(token_header) = headers.get("X-Api-Token") {
        if let Ok(token) = token_header.to_str() {
            return token == expected_token;
        }
    }

    false
}

fn attach_tool_metrics(agent: JiaClawAgent, metrics: &Arc<Metrics>) -> JiaClawAgent {
    let metrics = Arc::clone(metrics);
    agent.with_tool_metrics_hook(Arc::new(move |tool, ok| {
        metrics.record_tool_call(tool, ok);
    }))
}

/// 健康检查处理器
async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    let response = HealthResponse {
        status: "ok".to_string(),
        agent_name: state.agent.config().name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    Json(response)
}

/// Prometheus 文本指标。默认公开；`metrics_public = false` 或 `JIACLAW_METRICS_REQUIRE_AUTH=1` 时与 `/api/*` 相同鉴权。
async fn metrics_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    if state.metrics_require_auth && !check_api_auth(&state, &headers) {
        tracing::warn!(
            request_id = request_id_log_value(&headers),
            "Metrics 鉴权失败: token 不匹配或缺失"
        );
        return Err(AppError::Unauthorized);
    }

    let sessions_active = state
        .sessions
        .lock()
        .map(|guard| u64::try_from(guard.len()).unwrap_or(u64::MAX))
        .unwrap_or(0);
    let body = state
        .metrics
        .render(sessions_active, env!("CARGO_PKG_VERSION"));
    Ok((
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static(PROMETHEUS_CONTENT_TYPE),
        )],
        body,
    ))
}

/// `OpenAPI` 3 草图。鉴权与 `/api/tools` 一致：有 token 则需要鉴权。
async fn openapi_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    if !check_api_auth(&state, &headers) {
        tracing::warn!(
            request_id = request_id_log_value(&headers),
            "API 鉴权失败: token 不匹配或缺失"
        );
        return Err(AppError::Unauthorized);
    }

    Ok((
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        OPENAPI_JSON,
    ))
}

/// HTTP 聊天请求体：核心 `ChatRequest` + 可选 `stream`。
#[derive(Debug, Deserialize)]
struct ChatHttpBody {
    #[serde(flatten)]
    request: ChatRequest,
    /// 为 true 时返回 SSE；也可通过 `Accept: text/event-stream` 开启。
    #[serde(default)]
    stream: bool,
}

/// `Accept` 是否显式包含 `text/event-stream`（不含 `*/*`）。
fn accept_includes_event_stream(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|accept| {
            accept.split(',').any(|part| {
                part.split(';')
                    .next()
                    .map(str::trim)
                    .is_some_and(|media| media.eq_ignore_ascii_case("text/event-stream"))
            })
        })
}

fn wants_event_stream(headers: &HeaderMap, stream_field: bool) -> bool {
    stream_field || accept_includes_event_stream(headers)
}

fn sse_json_event(name: &str, data: &serde_json::Value) -> Event {
    Event::default().event(name).data(data.to_string())
}

fn tool_sse_payload(call: &ToolCall) -> serde_json::Value {
    let error = call
        .result
        .as_ref()
        .and_then(|v| v.get("error"))
        .and_then(serde_json::Value::as_str);
    match error {
        Some(error) => json!({
            "name": call.tool_name,
            "ok": false,
            "error": error,
        }),
        None => json!({
            "name": call.tool_name,
            "ok": true,
        }),
    }
}

/// 将助手文本按句/按块切开，供 SSE `token` 事件使用。
///
/// TODO(true-streaming): `Brokerrouter` / `StateKnot` 提供 token stream API 后改为真流式。
fn chunk_assistant_text(text: &str) -> Vec<String> {
    const MAX_CHARS: usize = 80;
    if text.is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_chars = 0usize;
    for ch in text.chars() {
        current.push(ch);
        current_chars += 1;
        let at_sentence = matches!(ch, '。' | '！' | '？' | '.' | '!' | '?' | '\n');
        if at_sentence || current_chars >= MAX_CHARS {
            chunks.push(std::mem::take(&mut current));
            current_chars = 0;
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn build_chat_sse_events(
    request_id: &str,
    session_id: Option<&str>,
    outcome: Result<&ChatResponse, &str>,
) -> Vec<Event> {
    let mut events = vec![sse_json_event(
        "meta",
        &json!({
            "session_id": session_id,
            "request_id": request_id,
        }),
    )];

    match outcome {
        Ok(response) => {
            for call in &response.tool_calls {
                events.push(sse_json_event("tool", &tool_sse_payload(call)));
            }
            // TODO(true-streaming): 底层 LLM 暂无 token stream；此处为整段生成后的分块推送。
            for chunk in chunk_assistant_text(&response.message.content) {
                events.push(sse_json_event("token", &json!({ "text": chunk })));
            }
            let status =
                serde_json::to_value(&response.status).unwrap_or_else(|_| json!("unknown"));
            events.push(sse_json_event(
                "done",
                &json!({
                    "reply": response.message.content,
                    "status": status,
                    "session_id": response.session_id,
                }),
            ));
        }
        Err(message) => {
            events.push(sse_json_event("error", &json!({ "error": message })));
        }
    }

    events
}

fn chat_sse_response(events: Vec<Event>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    Sse::new(stream::iter(events.into_iter().map(Ok)))
}

/// 聊天处理器
#[allow(clippy::too_many_lines)]
async fn chat_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ChatHttpBody>,
) -> Result<Response, AppError> {
    let request_id = request_id_log_value(&headers).to_string();
    let stream = wants_event_stream(&headers, body.stream);
    let mut request = body.request;

    // API Token 鉴权检查（失败仍返回 JSON 401，即使客户端请求了 SSE）
    if !check_api_auth(&state, &headers) {
        tracing::warn!(request_id = %request_id, "API 鉴权失败: token 不匹配或缺失");
        return Err(AppError::Unauthorized);
    }

    tracing::info!(
        request_id = %request_id,
        "收到聊天请求，消息数: {}, session_id: {:?}, stream: {stream}",
        request.messages.len(),
        request.session_id
    );

    let session_id = request.session_id.clone();

    purge_expired_sessions(&state);

    if let Some(ref sid) = session_id {
        request.messages = prepare_session_chat_messages(
            &state,
            sid,
            request.messages.clone(),
            &request_id,
            "http",
        )
        .await;
    }

    let response = match state.agent.chat(&request).await {
        Ok(response) => response,
        Err(e) => {
            let message = e.to_string();
            if stream {
                tracing::error!(request_id = %request_id, "聊天失败（SSE error 事件）: {message}");
                let events = build_chat_sse_events(
                    &request_id,
                    session_id.as_deref(),
                    Err(message.as_str()),
                );
                return Ok(chat_sse_response(events).into_response());
            }
            return Err(AppError::Internal(message));
        }
    };

    // 如果提供了 session_id，更新 session 历史
    if let Some(ref sid) = session_id {
        let mut messages = request.messages.clone();
        messages.push(response.message.clone());
        commit_session_messages(&state, sid, messages, &request_id, "http");
    }

    tracing::info!(
        request_id = %request_id,
        "聊天响应生成，状态: {:?}, 工具调用数: {}",
        response.status,
        response.tool_calls.len()
    );

    // 将 session_id 添加到响应中
    let mut response = response;
    response.session_id = session_id;

    if stream {
        let events =
            build_chat_sse_events(&request_id, response.session_id.as_deref(), Ok(&response));
        return Ok(chat_sse_response(events).into_response());
    }

    Ok(Json(response).into_response())
}

/// 会话列表项
#[derive(Debug, Serialize, Deserialize)]
struct SessionSummary {
    id: String,
    message_count: usize,
}

/// 列出会话响应
#[derive(Debug, Serialize, Deserialize)]
struct ListSessionsResponse {
    sessions: Vec<SessionSummary>,
}

/// 读取会话响应
#[derive(Debug, Serialize, Deserialize)]
struct GetSessionResponse {
    id: String,
    messages: Vec<ChatMessage>,
}

/// 创建会话响应
#[derive(Debug, Serialize, Deserialize)]
struct CreateSessionResponse {
    session_id: String,
}

/// 列出会话：读内存中的当前 store（落盘开启时也以内存为准，与 chat/delete 一致）。
async fn list_sessions_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ListSessionsResponse>, AppError> {
    if !check_api_auth(&state, &headers) {
        tracing::warn!(
            request_id = request_id_log_value(&headers),
            "API 鉴权失败: token 不匹配或缺失"
        );
        return Err(AppError::Unauthorized);
    }

    purge_expired_sessions(&state);

    let mut sessions: Vec<SessionSummary> = {
        let mut map = state.sessions.lock().unwrap();
        let now = Instant::now();
        for rec in map.values_mut() {
            rec.last_accessed = now;
        }
        map.iter()
            .map(|(id, rec)| SessionSummary {
                id: id.clone(),
                message_count: rec.messages.len(),
            })
            .collect()
    };
    sessions.sort_by(|a, b| a.id.cmp(&b.id));

    tracing::info!(
        request_id = request_id_log_value(&headers),
        "列出 sessions: {} 个",
        sessions.len()
    );
    Ok(Json(ListSessionsResponse { sessions }))
}

/// 读取会话历史；不存在返回 404。读内存 store，不重新扫盘。
async fn get_session_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<GetSessionResponse>, AppError> {
    if !check_api_auth(&state, &headers) {
        tracing::warn!(
            request_id = request_id_log_value(&headers),
            "API 鉴权失败: token 不匹配或缺失"
        );
        return Err(AppError::Unauthorized);
    }

    purge_expired_sessions(&state);

    let messages = {
        let mut map = state.sessions.lock().unwrap();
        map.get_mut(&session_id).map(|rec| {
            rec.touch();
            rec.messages.clone()
        })
    };

    if let Some(messages) = messages {
        tracing::info!(
            request_id = request_id_log_value(&headers),
            "读取 session {}，消息数: {}",
            session_id,
            messages.len()
        );
        Ok(Json(GetSessionResponse {
            id: session_id,
            messages,
        }))
    } else {
        tracing::info!(
            request_id = request_id_log_value(&headers),
            "session 不存在: {}",
            session_id
        );
        Err(AppError::NotFound)
    }
}

/// 导出会话查询参数。默认 JSONL；`format=json` 返回整包。
#[derive(Debug, Default, Deserialize)]
struct ExportSessionQuery {
    #[serde(default)]
    format: Option<String>,
}

fn wants_json_export(format: Option<&str>) -> bool {
    format.is_some_and(|value| value.eq_ignore_ascii_case("json"))
}

/// 将消息编码为 NDJSON（每行一条，沿用现有 `ChatMessage` 字段）。
fn encode_messages_jsonl(messages: &[ChatMessage]) -> Result<String, serde_json::Error> {
    let mut out = String::new();
    for message in messages {
        out.push_str(&serde_json::to_string(message)?);
        out.push('\n');
    }
    Ok(out)
}

/// 只读取出会话消息：过期先按 TTL 清理；不 touch、不摘要、不因导出而改写消息。
fn load_session_messages_for_export(
    state: &AppState,
    session_id: &str,
) -> Option<Vec<ChatMessage>> {
    purge_expired_sessions(state);
    let sessions = state.sessions.lock().unwrap();
    sessions.get(session_id).map(|rec| rec.messages.clone())
}

/// 导出会话历史；不存在或已过期返回 404。只读，不触发摘要、不刷新 TTL。
async fn export_session_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Query(query): Query<ExportSessionQuery>,
) -> Result<Response, AppError> {
    if !check_api_auth(&state, &headers) {
        tracing::warn!(
            request_id = request_id_log_value(&headers),
            "API 鉴权失败: token 不匹配或缺失"
        );
        return Err(AppError::Unauthorized);
    }

    let Some(messages) = load_session_messages_for_export(&state, &session_id) else {
        tracing::info!(
            request_id = request_id_log_value(&headers),
            "导出 session 不存在: {}",
            session_id
        );
        return Err(AppError::NotFound);
    };

    tracing::info!(
        request_id = request_id_log_value(&headers),
        "导出会话 {}，消息数: {}",
        session_id,
        messages.len()
    );

    if wants_json_export(query.format.as_deref()) {
        return Ok(Json(GetSessionResponse {
            id: session_id,
            messages,
        })
        .into_response());
    }

    let body = encode_messages_jsonl(&messages)
        .map_err(|e| AppError::Internal(format!("序列化会话导出失败: {e}")))?;
    Ok((
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static(SESSION_EXPORT_NDJSON),
        )],
        body,
    )
        .into_response())
}

/// 创建会话处理器
async fn create_session_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CreateSessionResponse>, AppError> {
    // API Token 鉴权检查
    if !check_api_auth(&state, &headers) {
        tracing::warn!("API 鉴权失败: token 不匹配或缺失");
        return Err(AppError::Unauthorized);
    }
    let session_id = uuid::Uuid::new_v4().to_string();

    purge_expired_sessions(&state);

    // 立即在 sessions map 中创建空的 Vec，这样后续 DELETE 能正确返回 success=true
    let mut sessions = state.sessions.lock().unwrap();
    sessions.insert(session_id.clone(), SessionRecord::new(Vec::new()));

    tracing::info!("创建新 session: {}", session_id);
    Ok(Json(CreateSessionResponse { session_id }))
}

/// 删除会话响应
#[derive(Debug, Serialize, Deserialize)]
struct DeleteSessionResponse {
    success: bool,
    message: String,
}

/// 工具信息
#[derive(Debug, Serialize, Deserialize)]
struct ToolInfo {
    name: String,
    description: String,
}

/// 工具列表响应
#[derive(Debug, Serialize, Deserialize)]
struct ToolsResponse {
    tools: Vec<ToolInfo>,
}

/// 技能信息
#[derive(Debug, Serialize, Deserialize)]
struct SkillInfo {
    name: String,
    description: String,
    path: String,
}

/// 技能列表响应
#[derive(Debug, Serialize, Deserialize)]
struct SkillsResponse {
    skills: Vec<SkillInfo>,
}

/// 删除会话处理器
async fn delete_session_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<DeleteSessionResponse>, AppError> {
    // API Token 鉴权检查
    if !check_api_auth(&state, &headers) {
        tracing::warn!("API 鉴权失败: token 不匹配或缺失");
        return Err(AppError::Unauthorized);
    }
    purge_expired_sessions(&state);

    let mut sessions = state.sessions.lock().unwrap();
    let existed = sessions.remove(&session_id).is_some();

    if existed {
        tracing::info!("删除 session: {}", session_id);

        persist_session_map(&state, &sessions);

        Ok(Json(DeleteSessionResponse {
            success: true,
            message: format!("会话 {session_id} 已删除"),
        }))
    } else {
        tracing::warn!("尝试删除不存在的 session: {}", session_id);
        Ok(Json(DeleteSessionResponse {
            success: false,
            message: format!("会话 {session_id} 不存在"),
        }))
    }
}

/// 工具列表处理器
async fn tools_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ToolsResponse>, AppError> {
    // API Token 鉴权检查
    if !check_api_auth(&state, &headers) {
        tracing::warn!("API 鉴权失败: token 不匹配或缺失");
        return Err(AppError::Unauthorized);
    }
    let tool_names = state.agent.tools().list();
    let tools: Vec<ToolInfo> = tool_names
        .iter()
        .filter_map(|name| {
            state.agent.tools().get(name).map(|tool| ToolInfo {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
            })
        })
        .collect();

    tracing::info!("列出工具: {} 个已注册", tools.len());
    Ok(Json(ToolsResponse { tools }))
}

/// 技能列表处理器
async fn skills_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SkillsResponse>, AppError> {
    // API Token 鉴权检查
    if !check_api_auth(&state, &headers) {
        tracing::warn!("API 鉴权失败: token 不匹配或缺失");
        return Err(AppError::Unauthorized);
    }
    let workspace_path = &state.agent.config().workspace_path;
    let discovery = jiaclaw::SkillDiscovery::new(workspace_path);

    let skills = match discovery.discover() {
        Ok(discovered_skills) => {
            tracing::info!("发现技能: {} 个", discovered_skills.len());
            discovered_skills
                .into_iter()
                .map(|skill| SkillInfo {
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                    path: skill.path.to_string_lossy().to_string(),
                })
                .collect()
        }
        Err(e) => {
            tracing::warn!("技能发现失败: {}", e);
            Vec::new()
        }
    };

    Ok(Json(SkillsResponse { skills }))
}

fn unauthorized_hook_response() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"ok": false, "error": "unauthorized"})),
    )
        .into_response()
}

/// 将用户文本写入指定 session 并跑一轮 agent chat（`/hooks/inbound`、`/hooks/telegram`、`/hooks/slack` 与 `/hooks/discord` 共用）。
async fn run_session_user_chat(
    state: &AppState,
    session_id: &str,
    user_text: &str,
    request_id: &str,
    channel_label: &str,
) -> Result<String, AppError> {
    tracing::info!(
        request_id = %request_id,
        "使用 {channel_label} session_id: {session_id}"
    );

    purge_expired_sessions(state);

    let incoming = vec![ChatMessage {
        role: MessageRole::User,
        content: user_text.to_string(),
    }];
    let request = ChatRequest {
        messages: prepare_session_chat_messages(
            state,
            session_id,
            incoming,
            request_id,
            channel_label,
        )
        .await,
        enabled_tools: vec![],
        enabled_skills: vec![],
        auto_skills: true,
        session_id: Some(session_id.to_string()),
    };

    let response = state
        .agent
        .chat(&request)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    {
        let mut messages = request.messages.clone();
        messages.push(response.message.clone());
        commit_session_messages(state, session_id, messages, request_id, channel_label);
    }

    tracing::info!(
        request_id = %request_id,
        "{channel_label} 聊天响应生成，状态: {:?}, 工具调用数: {}",
        response.status,
        response.tool_calls.len()
    );

    Ok(response.message.content)
}

/// Webhook 入站处理器
async fn hooks_inbound_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<InboundWebhookRequest>,
) -> Result<impl IntoResponse, AppError> {
    let request_id = request_id_log_value(&headers).to_string();

    tracing::info!(
        request_id = %request_id,
        "收到 webhook 入站请求: channel={}, chat_id={}, username={:?}",
        body.channel,
        body.chat_id,
        body.username
    );

    if let Some(expected) = state.webhook_secret.as_deref() {
        let provided = headers
            .get("X-Webhook-Secret")
            .and_then(|v| v.to_str().ok());
        if provided != Some(expected) {
            tracing::warn!(request_id = %request_id, "Webhook 鉴权失败: secret 不匹配");
            return Ok(unauthorized_hook_response());
        }
    }

    let session_id = format!("webhook:{}", body.chat_id);
    let reply =
        run_session_user_chat(&state, &session_id, &body.text, &request_id, "webhook").await?;

    let webhook_response = InboundWebhookResponse {
        ok: true,
        session_id,
        reply: Some(reply.clone()),
        message: Some(reply),
        error: None,
    };

    Ok((StatusCode::OK, Json(webhook_response)).into_response())
}

/// Telegram Bot 入站：把 Update 映射到现有 session/chat 路径。
async fn hooks_telegram_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(update): Json<TelegramUpdate>,
) -> Result<impl IntoResponse, AppError> {
    let request_id = request_id_log_value(&headers).to_string();

    tracing::info!(request_id = %request_id, "收到 Telegram 入站 Update");

    if let Some(expected) = state.telegram_secret.as_deref() {
        let provided = headers
            .get(X_TELEGRAM_BOT_API_SECRET_TOKEN)
            .and_then(|v| v.to_str().ok());
        if provided != Some(expected) {
            tracing::warn!(
                request_id = %request_id,
                "Telegram 鉴权失败: secret token 不匹配"
            );
            return Ok(unauthorized_hook_response());
        }
    }

    let Some((chat_id, text)) = telegram_inbound_text(&update) else {
        tracing::info!(request_id = %request_id, "跳过无文本的 Telegram update");
        let skipped = TelegramInboundResponse {
            ok: true,
            reply: None,
            session_id: None,
            skipped: Some(true),
            reason: Some("no text in update".to_string()),
            delivered: None,
            delivery_error: None,
        };
        return Ok((StatusCode::OK, Json(skipped)).into_response());
    };

    let session_id = format!("telegram:{chat_id}");
    let reply = run_session_user_chat(&state, &session_id, &text, &request_id, "telegram").await?;

    let (delivered, delivery_error) = if let Some(token) = state.telegram_bot_token.as_deref() {
        let delivery = send_telegram_reply(
            &state.telegram_api_base,
            token,
            &chat_id,
            &reply,
            &request_id,
        )
        .await;
        (Some(delivery.delivered), delivery.error)
    } else {
        (None, None)
    };

    let body = TelegramInboundResponse {
        ok: true,
        reply: Some(reply),
        session_id: Some(session_id),
        skipped: None,
        reason: None,
        delivered,
        delivery_error,
    };

    Ok((StatusCode::OK, Json(body)).into_response())
}

fn slack_skipped_response(reason: &str) -> Response {
    let skipped = SlackInboundResponse {
        ok: true,
        reply: None,
        session_id: None,
        skipped: Some(true),
        reason: Some(reason.to_string()),
        delivered: None,
        delivery_error: None,
    };
    (StatusCode::OK, Json(skipped)).into_response()
}

fn invalid_json_hook_response() -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"ok": false, "error": "invalid json"})),
    )
        .into_response()
}

/// Slack Events API 入站：先取 raw body 再反序列化，以便校验官方 v0 签名。
async fn hooks_slack_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let request_id = request_id_log_value(&headers).to_string();

    tracing::info!(request_id = %request_id, "收到 Slack Events 入站");

    if let Some(secret) = state.slack_signing_secret.as_deref() {
        let timestamp = headers
            .get(X_SLACK_REQUEST_TIMESTAMP)
            .and_then(|v| v.to_str().ok());
        let signature = headers.get(X_SLACK_SIGNATURE).and_then(|v| v.to_str().ok());
        if !verify_slack_request(secret, timestamp, signature, &body, unix_now_secs()) {
            tracing::warn!(
                request_id = %request_id,
                "Slack 鉴权失败: 签名或时间戳无效"
            );
            return Ok(unauthorized_hook_response());
        }
    }

    let envelope = match serde_json::from_slice::<SlackEnvelope>(&body) {
        Ok(envelope) => envelope,
        Err(err) => {
            tracing::warn!(
                request_id = %request_id,
                error = %err,
                "Slack 入站 JSON 无法解析"
            );
            return Ok(invalid_json_hook_response());
        }
    };

    match classify_slack_envelope(&envelope) {
        SlackInboundKind::UrlVerification { challenge } => {
            tracing::info!(request_id = %request_id, "Slack URL 验证 challenge 回传");
            Ok((StatusCode::OK, Json(SlackChallengeResponse { challenge })).into_response())
        }
        SlackInboundKind::Skipped { reason } => {
            tracing::info!(request_id = %request_id, reason = %reason, "跳过 Slack 事件");
            Ok(slack_skipped_response(&reason))
        }
        SlackInboundKind::Message {
            session_id,
            channel,
            text,
        } => {
            let reply =
                run_session_user_chat(&state, &session_id, &text, &request_id, "slack").await?;

            let (delivered, delivery_error) = if let Some(token) = state.slack_bot_token.as_deref()
            {
                let delivery =
                    send_slack_reply(&state.slack_api_base, token, &channel, &reply, &request_id)
                        .await;
                (Some(delivery.delivered), delivery.error)
            } else {
                (None, None)
            };

            let body = SlackInboundResponse {
                ok: true,
                reply: Some(reply),
                session_id: Some(session_id),
                skipped: None,
                reason: None,
                delivered,
                delivery_error,
            };
            Ok((StatusCode::OK, Json(body)).into_response())
        }
    }
}

/// Discord Interactions 入站：先取 raw body 再反序列化，以便校验官方 Ed25519 签名。
///
/// 立即返回 `type=5` deferred ACK（PING 为 `type=1`），后台跑 chat 后再编辑原始消息。
async fn hooks_discord_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let request_id = request_id_log_value(&headers).to_string();

    tracing::info!(request_id = %request_id, "收到 Discord Interactions 入站");

    if let Some(public_key) = state.discord_public_key.as_deref() {
        let timestamp = headers
            .get(X_SIGNATURE_TIMESTAMP)
            .and_then(|v| v.to_str().ok());
        let signature = headers
            .get(X_SIGNATURE_ED25519)
            .and_then(|v| v.to_str().ok());
        if !verify_discord_request(public_key, timestamp, signature, &body) {
            tracing::warn!(
                request_id = %request_id,
                "Discord 鉴权失败: Ed25519 签名无效"
            );
            return Ok(unauthorized_hook_response());
        }
    } else {
        tracing::debug!(
            request_id = %request_id,
            "Discord 公钥未配置，跳过签名校验（开发模式）"
        );
    }

    let interaction = match serde_json::from_slice::<DiscordInteraction>(&body) {
        Ok(interaction) => interaction,
        Err(err) => {
            tracing::warn!(
                request_id = %request_id,
                error = %err,
                "Discord 入站 JSON 无法解析"
            );
            return Ok(invalid_json_hook_response());
        }
    };

    match classify_discord_interaction(&interaction) {
        DiscordInboundKind::Ping => {
            tracing::info!(request_id = %request_id, "Discord Interactions PING → PONG");
            Ok(discord_pong_response())
        }
        DiscordInboundKind::Skipped { reason } => {
            tracing::info!(request_id = %request_id, reason = %reason, "跳过 Discord Interaction");
            Ok(discord_skipped_response(&reason))
        }
        DiscordInboundKind::ChatCommand {
            session_id,
            text,
            application_id,
            interaction_token,
        } => {
            tracing::info!(
                request_id = %request_id,
                session_id = %session_id,
                "Discord Chat Input Command deferred ACK"
            );
            spawn_discord_deferred_chat(
                state,
                session_id,
                text,
                application_id,
                interaction_token,
                request_id,
            );
            Ok(discord_deferred_response())
        }
    }
}

/// 应用错误类型
#[derive(Debug)]
enum AppError {
    Internal(String),
    Unauthorized,
    NotFound,
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            Self::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
            Self::NotFound => (StatusCode::NOT_FOUND, "session_not_found".to_string()),
        };

        let body = serde_json::json!({
            "error": message,
        });

        (status, Json(body)).into_response()
    }
}

/// 优雅关闭信号
async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.expect("等待 Ctrl+C 信号失败");
    tracing::info!("收到关闭信号，正在停止服务器...");
}

fn persist_path_from_config(config: &AgentConfig) -> PathBuf {
    if config.http.persist_path.starts_with('/') {
        PathBuf::from(&config.http.persist_path)
    } else {
        config.workspace_path.join(&config.http.persist_path)
    }
}

/// 从磁盘加载 sessions
fn load_sessions(path: &std::path::Path) -> HashMap<String, Vec<ChatMessage>> {
    if !path.exists() {
        tracing::info!("Session 文件不存在，从空 map 开始");
        return HashMap::new();
    }

    match std::fs::read_to_string(path) {
        Ok(content) => match serde_json::from_str::<HashMap<String, Vec<ChatMessage>>>(&content) {
            Ok(sessions) => {
                tracing::info!("成功加载 {} 个 sessions", sessions.len());
                sessions
            }
            Err(e) => {
                tracing::warn!("Session 文件损坏，无法解析: {}。从空 map 开始", e);
                HashMap::new()
            }
        },
        Err(e) => {
            tracing::warn!("无法读取 session 文件: {}。从空 map 开始", e);
            HashMap::new()
        }
    }
}

/// 保存 sessions 到磁盘（原子写入）
fn save_sessions(path: &PathBuf, sessions: &HashMap<String, Vec<ChatMessage>>) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("无法创建目录: {}", parent.display()))?;
    }

    let json = serde_json::to_string_pretty(sessions).context("序列化 sessions 失败")?;

    let temp_path = path.with_extension("tmp");
    std::fs::write(&temp_path, json)
        .with_context(|| format!("无法写入临时文件: {}", temp_path.display()))?;

    std::fs::rename(&temp_path, path).with_context(|| {
        format!(
            "无法重命名文件: {} -> {}",
            temp_path.display(),
            path.display()
        )
    })?;

    Ok(())
}

async fn chat_command(
    config_path: Option<PathBuf>,
    message: Option<&str>,
    skills: Vec<String>,
    session_id: Option<String>,
    no_auto_skill: bool,
) -> Result<()> {
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

    tracing::info!("使用 Agent 配置: {}", config.name);

    // 检查工作空间是否存在
    if !config.workspace_path.exists() {
        tracing::warn!("工作空间不存在，使用默认配置");
        println!("\n💡 提示: 运行 'jiaclaw init' 创建工作空间");
    }

    // 创建并使用 agent
    let agent = JiaClawAgent::new(config).context("创建 JiaClawAgent 失败")?;

    // 如果提供了消息，执行单次聊天
    if let Some(msg) = message {
        tracing::info!("运行单次聊天");
        single_chat(&agent, msg, &skills, session_id, no_auto_skill).await?;
    } else {
        // 否则进入 REPL 模式
        tracing::info!("进入 REPL 模式");
        repl_chat(&agent, &skills, session_id, no_auto_skill).await?;
    }

    Ok(())
}

/// 单次聊天模式
async fn single_chat(
    agent: &JiaClawAgent,
    message: &str,
    skills: &[String],
    session_id: Option<String>,
    no_auto_skill: bool,
) -> Result<()> {
    let request = ChatRequest {
        messages: vec![ChatMessage {
            role: MessageRole::User,
            content: message.to_string(),
        }],
        enabled_tools: vec![],
        enabled_skills: skills.to_vec(),
        auto_skills: !no_auto_skill,
        session_id: session_id.clone(),
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
                    println!(
                        "     结果: {}",
                        result_str
                            .lines()
                            .take(3)
                            .collect::<Vec<_>>()
                            .join("\n     ")
                    );
                    if result_str.lines().count() > 3 {
                        println!("     ...");
                    }
                } else {
                    // 结果是其他 JSON，格式化显示
                    println!(
                        "     结果: {}",
                        serde_json::to_string_pretty(result).unwrap_or_default()
                    );
                }
            }
        }
        println!();
    }

    println!("\n助手回复:");
    println!("{}", response.message.content);
    println!("\n状态: {:?}", response.status);

    if let Some(sid) = session_id {
        println!("会话 ID: {sid}");
    }

    Ok(())
}

/// REPL 多轮对话模式
async fn repl_chat(
    agent: &JiaClawAgent,
    skills: &[String],
    session_id: Option<String>,
    no_auto_skill: bool,
) -> Result<()> {
    use std::io::{self, Write};

    println!("\n🦀 JiaClaw REPL 模式");
    println!("输入消息开始对话，输入 'exit'、'quit' 或按 Ctrl+D 退出\n");

    if !skills.is_empty() {
        println!("✨ 启用的技能: {}", skills.join(", "));
    }
    if no_auto_skill {
        println!("⚠️  技能自动激活已禁用");
    }
    if let Some(ref sid) = session_id {
        println!("📝 会话 ID: {sid}");
    }
    println!();

    // 维护会话历史
    let mut history: Vec<ChatMessage> = Vec::new();
    let enabled_skills = skills.to_vec();

    loop {
        // 显示提示符
        print!("👤 > ");
        io::stdout().flush()?;

        // 读取用户输入
        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(0) => {
                // EOF (Ctrl+D)
                println!("\n👋 再见！");
                break;
            }
            Ok(_) => {
                let trimmed = input.trim();

                // 检查退出命令
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed == "exit" || trimmed == "quit" {
                    println!("👋 再见！");
                    break;
                }

                // 添加用户消息到历史
                let user_message = ChatMessage {
                    role: MessageRole::User,
                    content: trimmed.to_string(),
                };
                history.push(user_message);

                // 构建请求
                let request = ChatRequest {
                    messages: history.clone(),
                    enabled_tools: vec![],
                    enabled_skills: enabled_skills.clone(),
                    auto_skills: !no_auto_skill,
                    session_id: session_id.clone(),
                };

                // 调用 agent
                match agent.chat(&request).await {
                    Ok(response) => {
                        // 显示工具调用（如果有）
                        if !response.tool_calls.is_empty() {
                            println!("\n🔧 工具调用:");
                            for tool_call in &response.tool_calls {
                                println!("   • {}", tool_call.tool_name);
                            }
                            println!();
                        }

                        // 显示助手回复
                        println!("🤖 {}\n", response.message.content);

                        // 添加助手消息到历史
                        history.push(response.message);
                    }
                    Err(e) => {
                        eprintln!("❌ 错误: {e}\n");
                        // 失败时从历史中移除刚才的用户消息
                        history.pop();
                    }
                }
            }
            Err(e) => {
                eprintln!("❌ 读取输入失败: {e}");
                break;
            }
        }
    }

    Ok(())
}

fn version_command() {
    println!("JiaClaw v{}", env!("CARGO_PKG_VERSION"));
    println!("基于 StateKnot 框架构建");
    println!("许可证: Apache-2.0 OR MIT");
    println!("仓库: https://github.com/jiawenyao401/JiaClaw");
}

#[allow(clippy::too_many_lines)]
fn skills_command(config_path: Option<PathBuf>, verbose: bool) -> Result<()> {
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

    println!("🎯 JiaClaw 技能列表\n");
    println!("📁 工作空间: {}\n", config.workspace_path.display());

    let skills_dir = config.workspace_path.join("skills");

    if !skills_dir.exists() {
        println!("❌ 技能目录不存在: {}", skills_dir.display());
        println!("\n💡 运行 'jiaclaw init' 创建工作空间和示例技能");
        return Ok(());
    }

    let discovery = jiaclaw::SkillDiscovery::new(&config.workspace_path);

    match discovery.discover() {
        Ok(skills) => {
            if skills.is_empty() {
                println!("⚠️  未发现任何技能");
                println!(
                    "\n💡 在 {} 中创建技能目录和 SKILL.md 文件",
                    skills_dir.display()
                );
                println!("   每个技能应包含:");
                println!("   • YAML frontmatter（name, description, triggers）");
                println!("   • Markdown 内容（技能说明）");
            } else {
                println!("✅ 发现 {} 个技能:\n", skills.len());

                for (i, skill) in skills.iter().enumerate() {
                    println!("{}. {}", i + 1, skill.name);
                    println!("   描述: {}", skill.description);

                    if skill.triggers.is_empty() {
                        println!("   触发词: 无");
                    } else {
                        println!("   触发词: {}", skill.triggers.join(", "));
                    }

                    println!("   路径: {}", skill.path.display());

                    if verbose {
                        println!("\n   内容预览:");
                        let preview = skill
                            .content
                            .lines()
                            .take(5)
                            .collect::<Vec<_>>()
                            .join("\n   ");
                        println!("   {preview}");
                        if skill.content.lines().count() > 5 {
                            println!("   ...");
                        }
                    }

                    println!();
                }

                println!("💡 使用提示:");
                println!("   • 显式启用: 在 ChatRequest 的 enabled_skills 字段中指定");
                println!("   • 自动激活: 当用户消息包含触发词时自动启用");
                println!("   • 详细模式: 使用 --verbose 查看技能完整内容");
            }
        }
        Err(e) => {
            println!("❌ 技能发现失败: {e}");
            return Err(e.into());
        }
    }

    Ok(())
}

fn load_agent_config(config_path: Option<PathBuf>) -> Result<AgentConfig> {
    if let Some(path) = config_path {
        let path_str = path.to_string_lossy();
        if path_str.ends_with(".toml") {
            Ok(AgentConfig::from_toml_file(&path)?)
        } else if path_str.ends_with(".json") {
            Ok(AgentConfig::from_json_file(&path)?)
        } else {
            Ok(AgentConfig::from_toml_file(&path)
                .or_else(|_| AgentConfig::from_json_file(&path))?)
        }
    } else {
        Ok(AgentConfig::default())
    }
}

/// 从落盘 session store 只读导出 JSONL。不触发摘要、不改写文件。
fn session_export_command(
    config_path: Option<PathBuf>,
    session_id: &str,
    output: Option<&std::path::Path>,
) -> Result<()> {
    let config = load_agent_config(config_path)?;
    let persist_path = persist_path_from_config(&config);
    export_session_from_persist_file(&persist_path, session_id, output)
}

fn export_session_from_persist_file(
    persist_path: &std::path::Path,
    session_id: &str,
    output: Option<&std::path::Path>,
) -> Result<()> {
    let sessions = load_sessions(persist_path);
    let Some(messages) = sessions.get(session_id) else {
        anyhow::bail!("session_not_found");
    };
    write_session_jsonl(messages, output)
}

fn write_session_jsonl(messages: &[ChatMessage], output: Option<&std::path::Path>) -> Result<()> {
    let body = encode_messages_jsonl(messages).context("序列化会话导出失败")?;
    if let Some(path) = output {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("无法创建目录: {}", parent.display()))?;
            }
        }
        std::fs::write(path, body).with_context(|| format!("无法写入 {}", path.display()))?;
    } else {
        print!("{body}");
    }
    Ok(())
}

fn memory_show_command(config_path: Option<PathBuf>) -> Result<()> {
    let config = load_agent_config(config_path)?;
    let status = inspect_memory_file(&config.workspace_path, &config.memory.path)
        .context("无法解析 MEMORY 路径")?;

    println!("🧠 长期记忆\n");
    println!("   工作空间: {}", config.workspace_path.display());
    println!("   约定路径: {}", config.memory.path);
    println!("   文件:     {}", status.path.display());

    if !status.exists {
        println!("   状态:     ⚠️  不存在 (0 bytes)");
        println!("\n💡 文件缺失时对话仍可进行，不会报错。");
        println!("   可手动创建该文件，或让 Agent 调用 memory_append。");
        return Ok(());
    }

    println!("   状态:     ✅ 存在 ({} bytes)", status.size_bytes);

    let content = std::fs::read_to_string(&status.path)
        .with_context(|| format!("无法读取 {}", status.path.display()))?;

    if content.trim().is_empty() {
        println!("\n（文件为空，不会注入系统提示）");
        return Ok(());
    }

    println!("\n----- 内容 -----\n{content}");
    if !content.ends_with('\n') {
        println!();
    }
    println!("----- 结束 -----");
    Ok(())
}

#[derive(Clone, Copy)]
enum IdentityShowKind {
    Soul,
    User,
}

fn identity_show_command(config_path: Option<PathBuf>, kind: IdentityShowKind) -> Result<()> {
    let config = load_agent_config(config_path)?;
    let (title, rel_path, tool_hint) = match kind {
        IdentityShowKind::Soul => (
            "✨ 人格（SOUL）",
            config.identity.soul_path.as_str(),
            "soul_write",
        ),
        IdentityShowKind::User => (
            "👤 用户画像（USER）",
            config.identity.user_path.as_str(),
            "user_write",
        ),
    };
    let status =
        inspect_identity_file(&config.workspace_path, rel_path).context("无法解析身份文件路径")?;

    println!("{title}\n");
    println!("   工作空间: {}", config.workspace_path.display());
    println!("   约定路径: {rel_path}");
    println!("   文件:     {}", status.path.display());

    if !status.exists {
        println!("   状态:     ⚠️  不存在 (0 bytes)");
        println!("\n💡 文件缺失时对话仍可进行，不会报错。");
        println!("   可手动创建该文件，或让 Agent 调用 {tool_hint}。");
        return Ok(());
    }

    println!("   状态:     ✅ 存在 ({} bytes)", status.size_bytes);

    let content = std::fs::read_to_string(&status.path)
        .with_context(|| format!("无法读取 {}", status.path.display()))?;

    if content.trim().is_empty() {
        println!("\n（文件为空，不会注入系统提示）");
        return Ok(());
    }

    println!("\n----- 内容 -----\n{content}");
    if !content.ends_with('\n') {
        println!();
    }
    println!("----- 结束 -----");
    Ok(())
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

    let mut workspace_ok = false;
    let mut workspace_file_count = 0;
    let mut skills_count = 0;

    if config.workspace_path.exists() {
        println!("   状态: ✅ 存在");

        match Workspace::load(&config.workspace_path) {
            Ok(workspace) => {
                let mut files = Vec::new();
                if workspace.agents.is_some() {
                    files.push("AGENTS.md");
                    workspace_file_count += 1;
                }
                if workspace.soul.is_some() {
                    files.push("SOUL.md");
                    workspace_file_count += 1;
                }
                if workspace.user.is_some() {
                    files.push("USER.md");
                    workspace_file_count += 1;
                }
                if workspace.memory.is_some() {
                    files.push("MEMORY.md");
                    workspace_file_count += 1;
                }

                if files.is_empty() {
                    println!("   ⚠️  没有找到工作空间文件");
                    println!("   💡 运行 'jiaclaw init' 创建默认文件");
                } else {
                    println!("   文件: {} 个已加载 ({})", files.len(), files.join(", "));
                    workspace_ok = true;
                }

                match inspect_memory_file(&config.workspace_path, &config.memory.path) {
                    Ok(status) => {
                        println!(
                            "   MEMORY: {} （配置路径: {}）",
                            status.path.display(),
                            config.memory.path
                        );
                        if status.exists {
                            println!("           ✅ 存在 ({} bytes)", status.size_bytes);
                        } else {
                            println!("           ⚠️  不存在");
                            println!("           💡 可手动创建，或在对话中使用 memory_append 写入");
                        }
                    }
                    Err(e) => {
                        println!("   MEMORY: ❌ 无法解析约定路径: {e}");
                    }
                }

                match inspect_identity_file(&config.workspace_path, &config.identity.soul_path) {
                    Ok(status) => {
                        println!(
                            "   SOUL:   {} （配置路径: {}）",
                            status.path.display(),
                            config.identity.soul_path
                        );
                        if status.exists {
                            println!("           ✅ 存在 ({} bytes)", status.size_bytes);
                        } else {
                            println!("           ⚠️  不存在");
                            println!("           💡 可手动创建，或在对话中使用 soul_write 写入");
                        }
                    }
                    Err(e) => {
                        println!("   SOUL:   ❌ 无法解析约定路径: {e}");
                    }
                }

                match inspect_identity_file(&config.workspace_path, &config.identity.user_path) {
                    Ok(status) => {
                        println!(
                            "   USER:   {} （配置路径: {}）",
                            status.path.display(),
                            config.identity.user_path
                        );
                        if status.exists {
                            println!("           ✅ 存在 ({} bytes)", status.size_bytes);
                        } else {
                            println!("           ⚠️  不存在");
                            println!("           💡 可手动创建，或在对话中使用 user_write 写入");
                        }
                    }
                    Err(e) => {
                        println!("   USER:   ❌ 无法解析约定路径: {e}");
                    }
                }

                match inspect_heartbeat_file(&config.workspace_path, &config.heartbeat.path) {
                    Ok(status) => {
                        println!(
                            "   HEARTBEAT: {} （配置路径: {}）",
                            status.path.display(),
                            config.heartbeat.path
                        );
                        if status.exists {
                            println!("           ✅ 存在 ({} bytes)", status.size_bytes);
                        } else {
                            println!("           ⚠️  不存在");
                            println!(
                                "           💡 启用 [heartbeat] 后，缺失或空文件会跳过本轮（不退出）"
                            );
                        }
                    }
                    Err(e) => {
                        println!("   HEARTBEAT: ❌ 无法解析约定路径: {e}");
                    }
                }

                // 检查技能
                let skills_dir = config.workspace_path.join("skills");
                if skills_dir.exists() {
                    let discovery = jiaclaw::SkillDiscovery::new(&config.workspace_path);
                    match discovery.discover() {
                        Ok(skills) => {
                            skills_count = skills.len();
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
            }
            Err(e) => {
                println!("   ⚠️  加载工作空间失败: {e}");
            }
        }
    } else {
        println!("   状态: ❌ 不存在");
        println!("   💡 运行 'jiaclaw init' 创建工作空间");
    }

    // 2. 检查工具系统
    println!("\n🔧 工具系统");
    let tools_count = match JiaClawAgent::new(config.clone()) {
        Ok(agent) => {
            let tool_list = agent.tools().list();
            let count = tool_list.len();
            println!("   已注册工具: {count} 个");
            for tool_name in tool_list {
                if let Some(tool) = agent.tools().get(tool_name) {
                    println!("      • {}: {}", tool.name(), tool.description());
                }
            }
            count
        }
        Err(e) => {
            println!("   ⚠️  无法初始化 Agent: {e}");
            0
        }
    };

    let tool_timeout_secs = config.effective_tool_timeout_secs();
    if let Some(secs) = tool_timeout_secs {
        println!(
            "   工具超时: ✅ 已启用（每调用 {secs} 秒，通过 {}）",
            tool_timeout_config_source()
        );
    } else {
        println!("   工具超时: ⚠️  未启用（不限制）");
        println!("   💡 设置环境变量: export JIACLAW_TOOL_TIMEOUT_SECS=30");
    }

    println!(
        "   工具循环上限: {}（范围 {}–{}；可用 JIACLAW_MAX_TOOL_ITERATIONS 覆盖）",
        max_tool_iterations_status_line(&config),
        MIN_MAX_TOOL_ITERATIONS,
        MAX_MAX_TOOL_ITERATIONS
    );

    let web_search_lines = web_search_status_lines(&config);
    if config.tools.web_search.enabled
        && config.tools.web_search.effective_brave_api_key().is_some()
    {
        println!("   web_search: ✅ {}", web_search_lines[0]);
    } else {
        println!("   web_search: ⚠️  {}", web_search_lines[0]);
        for hint in web_search_lines.iter().skip(1) {
            println!("   {hint}");
        }
    }

    if config.tools.web_fetch.enabled {
        println!("   web_fetch: ✅ {}", web_fetch_status_line(&config));
    } else {
        println!("   web_fetch: ⚠️  {}", web_fetch_status_line(&config));
    }

    if config.tools.memory_search.enabled {
        println!(
            "   memory_search: ✅ {}",
            memory_search_status_line(&config)
        );
    } else {
        println!(
            "   memory_search: ⚠️  {}",
            memory_search_status_line(&config)
        );
    }

    // 3. 检查提供商配置
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

    // 4. HTTP 配置检查
    println!("\n🌐 HTTP 配置");
    println!("   绑定地址: {}", config.http.bind);

    // 检查 API token（环境变量优先）
    let api_token = std::env::var("JIACLAW_API_TOKEN")
        .ok()
        .or(config.http.api_token.clone());

    if api_token.is_some() {
        println!(
            "   API 鉴权: ✅ 已启用（通过 {}）",
            if std::env::var("JIACLAW_API_TOKEN").is_ok() {
                "环境变量 JIACLAW_API_TOKEN"
            } else {
                "配置文件"
            }
        );
    } else {
        println!("   API 鉴权: ⚠️  未启用（本地开发友好）");
        println!("   💡 生产环境建议设置: export JIACLAW_API_TOKEN=your-token");
    }

    // 检查 webhook secret（环境变量优先）
    let webhook_secret = std::env::var("JIACLAW_WEBHOOK_SECRET")
        .ok()
        .or(config.http.webhook_secret.clone());

    if webhook_secret.is_some() {
        println!(
            "   Webhook 鉴权: ✅ 已启用（通过 {}）",
            if std::env::var("JIACLAW_WEBHOOK_SECRET").is_ok() {
                "环境变量 JIACLAW_WEBHOOK_SECRET"
            } else {
                "配置文件"
            }
        );
    } else {
        println!("   Webhook 鉴权: ⚠️  未启用");
        println!("   💡 设置环境变量: export JIACLAW_WEBHOOK_SECRET=your-secret");
    }

    // 检查 Telegram secret token（环境变量优先）
    let telegram_secret = std::env::var("JIACLAW_TELEGRAM_SECRET")
        .ok()
        .or(config.http.telegram_secret.clone());

    if telegram_secret.is_some() {
        println!(
            "   Telegram 鉴权: ✅ 已启用（通过 {}）",
            if std::env::var("JIACLAW_TELEGRAM_SECRET").is_ok() {
                "环境变量 JIACLAW_TELEGRAM_SECRET"
            } else {
                "配置文件"
            }
        );
    } else {
        println!("   Telegram 鉴权: ⚠️  未启用");
        println!("   💡 设置环境变量: export JIACLAW_TELEGRAM_SECRET=your-secret-token");
    }

    // 检查 Telegram Bot API token（环境变量优先，不打印明文）
    let telegram_bot_token = config.http.effective_telegram_bot_token();
    if telegram_bot_token.is_some() {
        println!(
            "   Telegram Bot Token: ✅ 已配置（通过 {}，明文不打印）",
            telegram_token_config_source()
        );
    } else {
        println!(
            "   Telegram Bot Token: ⚠️  未配置（/hooks/telegram 仅同步 JSON，不 sendMessage）"
        );
        println!("   💡 设置环境变量: export JIACLAW_TELEGRAM_BOT_TOKEN=your-bot-token");
    }

    let slack_signing_secret = config.http.effective_slack_signing_secret();
    if slack_signing_secret.is_some() {
        println!(
            "   Slack 签名校验: ✅ 已启用（通过 {}）",
            slack_signing_secret_config_source()
        );
    } else {
        println!("   Slack 签名校验: ⚠️  未启用");
        println!("   💡 设置环境变量: export JIACLAW_SLACK_SIGNING_SECRET=your-signing-secret");
    }

    let slack_bot_token = config.http.effective_slack_bot_token();
    if slack_bot_token.is_some() {
        println!(
            "   Slack Bot Token: ✅ 已配置（通过 {}，明文不打印）",
            slack_token_config_source()
        );
    } else {
        println!("   Slack Bot Token: ⚠️  未配置（/hooks/slack 仅同步 JSON，不 chat.postMessage）");
        println!("   💡 设置环境变量: export JIACLAW_SLACK_BOT_TOKEN=xoxb-your-bot-token");
    }

    let discord_public_key = config.http.effective_discord_public_key();
    if discord_public_key.is_some() {
        println!(
            "   Discord 签名校验: ✅ 已启用（通过 {}）",
            discord_public_key_config_source()
        );
    } else {
        println!("   Discord 签名校验: ⚠️  未启用");
        println!("   💡 设置环境变量: export JIACLAW_DISCORD_PUBLIC_KEY=your-hex-public-key");
    }

    let discord_bot_token = config.http.effective_discord_bot_token();
    if discord_bot_token.is_some() {
        println!(
            "   Discord Bot Token: ✅ 已配置（通过 {}，明文不打印）",
            discord_token_config_source()
        );
    } else {
        println!(
            "   Discord Bot Token: ⚠️  未配置（/hooks/discord deferred 后仅记 session，不 follow-up）"
        );
        println!("   💡 设置环境变量: export JIACLAW_DISCORD_BOT_TOKEN=your-bot-token");
    }

    let rate_limit_per_minute = config.http.effective_rate_limit_per_minute();
    if let Some(limit) = rate_limit_per_minute {
        println!(
            "   HTTP 限流: ✅ 已启用（{limit} 次/分钟，通过 {}）",
            rate_limit_config_source()
        );
    } else {
        println!("   HTTP 限流: ⚠️  未启用");
        println!("   💡 设置环境变量: export JIACLAW_RATE_LIMIT_PER_MINUTE=60");
    }

    let metrics_public = config.http.effective_metrics_public();
    if metrics_public {
        println!("   Metrics: ✅ 公开（GET /metrics 无需鉴权，不计入限流）");
    } else {
        println!(
            "   Metrics: 🔒 需 API 鉴权（与 /api/* 相同，通过 {}，不计入限流）",
            metrics_auth_config_source()
        );
        println!(
            "   💡 默认公开以便 scrape；生产若暴露公网可设 [http] metrics_public = false 或 JIACLAW_METRICS_REQUIRE_AUTH=1"
        );
    }

    let session_ttl_secs = config.http.effective_session_ttl_secs();
    if let Some(ttl) = session_ttl_secs {
        println!(
            "   Session TTL: ✅ 已启用（闲置 {ttl} 秒，通过 {}）",
            session_ttl_config_source()
        );
    } else {
        println!("   Session TTL: ⚠️  未启用");
        println!("   💡 设置环境变量: export JIACLAW_SESSION_TTL_SECS=3600");
    }

    if config.session.effective_summarize_on_overflow() {
        println!(
            "   Session 摘要压缩: ✅ {}",
            session_summarize_status_line(&config)
        );
    } else {
        println!(
            "   Session 摘要压缩: ⚠️  {}",
            session_summarize_status_line(&config)
        );
        println!(
            "   💡 设置 [session] summarize_on_overflow = true，或 export JIACLAW_SESSION_SUMMARIZE_ON_OVERFLOW=1"
        );
    }

    let heartbeat_interval_secs = config.heartbeat.effective_interval_secs();
    if config.heartbeat.enabled {
        println!(
            "   Heartbeat: ✅ 已启用（间隔 {heartbeat_interval_secs} 秒，session={}, 文件 {}，间隔来自 {}）",
            config.heartbeat.effective_session_id(),
            heartbeat_file_status_label(&config),
            heartbeat_interval_config_source()
        );
    } else {
        println!("   Heartbeat: ⚠️  未启用（默认关闭；仅 jiaclaw serve 会挂后台任务）");
        println!(
            "   💡 在配置中设置 [heartbeat] enabled = true，可选 JIACLAW_HEARTBEAT_INTERVAL_SECS"
        );
    }

    // CORS 配置
    if config.http.cors_allow_origins.is_empty()
        || (config.http.cors_allow_origins.len() == 1 && config.http.cors_allow_origins[0] == "*")
    {
        println!("   CORS 模式: 允许所有来源（Permissive）");
    } else {
        println!(
            "   CORS 模式: 限制来源（{}）",
            config.http.cors_allow_origins.join(", ")
        );
    }

    // 5. StateKnot 集成状态
    println!("\n⚙️  StateKnot 集成");
    println!("   状态: ⏳ 等待稳定 API 发布");
    println!("   持久化: ❌ 未启用");
    println!("   PostgreSQL: ❌ 未配置");
    println!("   💡 参见 docs/stateknot-gaps.md 了解详情");

    // 6. 总结
    println!("\n📊 总结");
    println!("   • 工作空间文件: {workspace_file_count} 个");
    println!("   • 已发现技能: {skills_count} 个");
    println!("   • 已注册工具: {tools_count} 个");
    println!("   • HTTP 绑定: {}", config.http.bind);
    println!(
        "   • API 鉴权: {}",
        if api_token.is_some() {
            "已启用"
        } else {
            "未启用"
        }
    );
    println!(
        "   • Webhook 鉴权: {}",
        if webhook_secret.is_some() {
            "已启用"
        } else {
            "未启用"
        }
    );
    println!(
        "   • Telegram 鉴权: {}",
        if telegram_secret.is_some() {
            "已启用"
        } else {
            "未启用"
        }
    );
    println!(
        "   • Telegram Bot Token: {}",
        if telegram_bot_token.is_some() {
            "已配置"
        } else {
            "未配置"
        }
    );
    println!(
        "   • Slack 签名校验: {}",
        if slack_signing_secret.is_some() {
            "已启用"
        } else {
            "未启用"
        }
    );
    println!(
        "   • Slack Bot Token: {}",
        if slack_bot_token.is_some() {
            "已配置"
        } else {
            "未配置"
        }
    );
    println!(
        "   • Discord 签名校验: {}",
        if discord_public_key.is_some() {
            "已启用"
        } else {
            "未启用"
        }
    );
    println!(
        "   • Discord Bot Token: {}",
        if discord_bot_token.is_some() {
            "已配置"
        } else {
            "未配置"
        }
    );
    println!(
        "   • HTTP 限流: {}",
        if let Some(limit) = rate_limit_per_minute {
            format!("已启用（{limit} 次/分钟）")
        } else {
            "未启用".to_string()
        }
    );
    println!(
        "   • Metrics: {}",
        if metrics_public {
            "公开（无需鉴权，不限流）"
        } else {
            "需 API 鉴权（不限流）"
        }
    );
    println!(
        "   • Session TTL: {}",
        if let Some(ttl) = session_ttl_secs {
            format!("已启用（闲置 {ttl} 秒）")
        } else {
            "未启用".to_string()
        }
    );
    println!(
        "   • Session 摘要压缩: {}",
        session_summarize_status_line(&config)
    );
    println!(
        "   • 工具超时: {}",
        if let Some(secs) = tool_timeout_secs {
            format!("已启用（每调用 {secs} 秒）")
        } else {
            "未启用".to_string()
        }
    );
    println!(
        "   • 工具循环上限: {}",
        max_tool_iterations_status_line(&config)
    );
    println!(
        "   • Heartbeat: {}",
        if config.heartbeat.enabled {
            format!(
                "已启用（间隔 {heartbeat_interval_secs} 秒，文件 {}）",
                heartbeat_file_status_label(&config)
            )
        } else {
            "未启用".to_string()
        }
    );

    if !has_key {
        println!("\n   ⚠️  运行模式: Stub（存根模式）");
        println!("   💡 未检测到 Brokerrouter/API key，将使用演示模式");
        println!("   💡 配置 JIACLAW_API_KEY 环境变量以启用真实模型调用");
    }

    if workspace_ok {
        println!("\n   ✅ 配置良好，可以开始使用");
        println!("   💡 试试: jiaclaw serve");
    } else {
        println!("\n   ⚠️  需要初始化工作空间");
        println!("   💡 运行: jiaclaw init");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use jiaclaw::SESSION_SUMMARY_PREFIX;
    use jiaclaw_core::{
        ChatMessage, ChatRequest, HeartbeatConfig, MemorySearchToolConfig, MessageRole,
        SessionConfig, ToolsConfig, WebFetchToolConfig, WebSearchToolConfig,
    };
    use tower::ServiceExt;

    fn create_test_app() -> Router {
        create_test_app_with_auth(None, None)
    }

    fn create_test_app_with_secret(webhook_secret: Option<String>) -> Router {
        create_test_app_with_auth(None, webhook_secret)
    }

    fn create_test_app_with_auth(
        api_token: Option<String>,
        webhook_secret: Option<String>,
    ) -> Router {
        create_test_app_with_options(api_token, webhook_secret, None)
    }

    fn create_test_app_with_rate_limit(per_minute: u32) -> Router {
        create_test_app_with_options(None, None, Some(per_minute))
    }

    fn create_test_app_with_options(
        api_token: Option<String>,
        webhook_secret: Option<String>,
        rate_limit_per_minute: Option<u32>,
    ) -> Router {
        create_test_app_with_full(api_token, webhook_secret, rate_limit_per_minute, None)
    }

    fn create_test_app_with_session_ttl(session_ttl_secs: Option<u64>) -> Router {
        create_test_app_with_full(None, None, None, session_ttl_secs)
    }

    fn create_test_app_with_telegram_secret(telegram_secret: Option<String>) -> Router {
        let config = AgentConfig::default();
        let agent = JiaClawAgent::new(config.clone()).expect("创建测试 agent 失败");
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));
        let state = AppState {
            agent: Arc::new(agent),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            api_token: None,
            webhook_secret: None,
            telegram_secret,
            telegram_bot_token: None,
            telegram_api_base: TELEGRAM_API_BASE.to_string(),
            slack_signing_secret: None,
            slack_bot_token: None,
            slack_api_base: SLACK_API_BASE.to_string(),
            discord_public_key: None,
            discord_bot_token: None,
            discord_api_base: DISCORD_API_BASE.to_string(),
            persist_enabled: false,
            persist_path: Arc::new(persist_path),
            rate_limiter: None,
            session_ttl: None,
            metrics: Arc::new(Metrics::default()),
            metrics_require_auth: false,
        };

        build_router(state)
    }

    #[derive(Clone)]
    struct TelegramApiMockState {
        status: StatusCode,
        captured: Arc<Mutex<Vec<CapturedTelegramOutbound>>>,
    }

    #[derive(Debug, Clone)]
    struct CapturedTelegramOutbound {
        method: String,
        path: String,
        body: serde_json::Value,
    }

    async fn telegram_api_mock_fallback(
        State(state): State<TelegramApiMockState>,
        req: axum::extract::Request,
    ) -> impl IntoResponse {
        let method = req.method().to_string();
        let path = req.uri().path().to_string();
        let bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
            .await
            .unwrap_or_default();
        let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
        state
            .captured
            .lock()
            .unwrap()
            .push(CapturedTelegramOutbound { method, path, body });
        let ok = state.status.is_success();
        (state.status, Json(json!({ "ok": ok })))
    }

    async fn spawn_telegram_api_mock(
        status: StatusCode,
    ) -> (String, Arc<Mutex<Vec<CapturedTelegramOutbound>>>) {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let state = TelegramApiMockState {
            status,
            captured: captured.clone(),
        };
        let app = Router::new()
            .fallback(telegram_api_mock_fallback)
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind telegram mock");
        let addr = listener.local_addr().expect("telegram mock local_addr");
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("telegram mock serve");
        });
        (format!("http://{addr}"), captured)
    }

    fn create_test_app_with_telegram_outbound(
        bot_token: Option<String>,
        api_base: String,
    ) -> Router {
        let config = AgentConfig::default();
        let agent = JiaClawAgent::new(config.clone()).expect("创建测试 agent 失败");
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));
        let state = AppState {
            agent: Arc::new(agent),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            api_token: None,
            webhook_secret: None,
            telegram_secret: None,
            telegram_bot_token: bot_token,
            telegram_api_base: api_base,
            slack_signing_secret: None,
            slack_bot_token: None,
            slack_api_base: SLACK_API_BASE.to_string(),
            discord_public_key: None,
            discord_bot_token: None,
            discord_api_base: DISCORD_API_BASE.to_string(),
            persist_enabled: false,
            persist_path: Arc::new(persist_path),
            rate_limiter: None,
            session_ttl: None,
            metrics: Arc::new(Metrics::default()),
            metrics_require_auth: false,
        };
        build_router(state)
    }

    fn json_chat_id(body: &serde_json::Value) -> Option<String> {
        body.get("chat_id").and_then(|value| {
            value
                .as_str()
                .map(ToString::to_string)
                .or_else(|| value.as_i64().map(|n| n.to_string()))
                .or_else(|| value.as_u64().map(|n| n.to_string()))
        })
    }

    fn json_channel(body: &serde_json::Value) -> Option<String> {
        body.get("channel")
            .and_then(|value| value.as_str().map(ToString::to_string))
    }

    fn create_test_app_with_slack_signing_secret(signing_secret: Option<String>) -> Router {
        let config = AgentConfig::default();
        let agent = JiaClawAgent::new(config.clone()).expect("创建测试 agent 失败");
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));
        let state = AppState {
            agent: Arc::new(agent),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            api_token: None,
            webhook_secret: None,
            telegram_secret: None,
            telegram_bot_token: None,
            telegram_api_base: TELEGRAM_API_BASE.to_string(),
            slack_signing_secret: signing_secret,
            slack_bot_token: None,
            slack_api_base: SLACK_API_BASE.to_string(),
            discord_public_key: None,
            discord_bot_token: None,
            discord_api_base: DISCORD_API_BASE.to_string(),
            persist_enabled: false,
            persist_path: Arc::new(persist_path),
            rate_limiter: None,
            session_ttl: None,
            metrics: Arc::new(Metrics::default()),
            metrics_require_auth: false,
        };
        build_router(state)
    }

    fn create_test_app_with_slack_outbound(bot_token: Option<String>, api_base: String) -> Router {
        let config = AgentConfig::default();
        let agent = JiaClawAgent::new(config.clone()).expect("创建测试 agent 失败");
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));
        let state = AppState {
            agent: Arc::new(agent),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            api_token: None,
            webhook_secret: None,
            telegram_secret: None,
            telegram_bot_token: None,
            telegram_api_base: TELEGRAM_API_BASE.to_string(),
            slack_signing_secret: None,
            slack_bot_token: bot_token,
            slack_api_base: api_base,
            discord_public_key: None,
            discord_bot_token: None,
            discord_api_base: DISCORD_API_BASE.to_string(),
            persist_enabled: false,
            persist_path: Arc::new(persist_path),
            rate_limiter: None,
            session_ttl: None,
            metrics: Arc::new(Metrics::default()),
            metrics_require_auth: false,
        };
        build_router(state)
    }

    #[derive(Clone)]
    struct SlackApiMockState {
        status: StatusCode,
        captured: Arc<Mutex<Vec<CapturedSlackOutbound>>>,
    }

    #[derive(Debug, Clone)]
    struct CapturedSlackOutbound {
        method: String,
        path: String,
        authorization: String,
        body: serde_json::Value,
    }

    async fn slack_api_mock_fallback(
        State(state): State<SlackApiMockState>,
        req: axum::extract::Request,
    ) -> impl IntoResponse {
        let method = req.method().to_string();
        let path = req.uri().path().to_string();
        let authorization = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
            .await
            .unwrap_or_default();
        let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
        state.captured.lock().unwrap().push(CapturedSlackOutbound {
            method,
            path,
            authorization,
            body,
        });
        let ok = state.status.is_success();
        (state.status, Json(json!({ "ok": ok })))
    }

    async fn spawn_slack_api_mock(
        status: StatusCode,
    ) -> (String, Arc<Mutex<Vec<CapturedSlackOutbound>>>) {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let state = SlackApiMockState {
            status,
            captured: captured.clone(),
        };
        let app = Router::new()
            .fallback(slack_api_mock_fallback)
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind slack mock");
        let addr = listener.local_addr().expect("slack mock local_addr");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("slack mock serve");
        });
        (format!("http://{addr}"), captured)
    }

    fn create_test_app_with_discord(
        public_key: Option<String>,
        bot_token: Option<String>,
        api_base: String,
    ) -> Router {
        let config = AgentConfig::default();
        let agent = JiaClawAgent::new(config.clone()).expect("创建测试 agent 失败");
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));
        let state = AppState {
            agent: Arc::new(agent),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            api_token: None,
            webhook_secret: None,
            telegram_secret: None,
            telegram_bot_token: None,
            telegram_api_base: TELEGRAM_API_BASE.to_string(),
            slack_signing_secret: None,
            slack_bot_token: None,
            slack_api_base: SLACK_API_BASE.to_string(),
            discord_public_key: public_key,
            discord_bot_token: bot_token,
            discord_api_base: api_base,
            persist_enabled: false,
            persist_path: Arc::new(persist_path),
            rate_limiter: None,
            session_ttl: None,
            metrics: Arc::new(Metrics::default()),
            metrics_require_auth: false,
        };
        build_router(state)
    }

    #[derive(Clone)]
    struct DiscordApiMockState {
        status: StatusCode,
        captured: Arc<Mutex<Vec<CapturedDiscordOutbound>>>,
    }

    #[derive(Debug, Clone)]
    struct CapturedDiscordOutbound {
        method: String,
        path: String,
        authorization: String,
        body: serde_json::Value,
    }

    async fn discord_api_mock_fallback(
        State(state): State<DiscordApiMockState>,
        req: axum::extract::Request,
    ) -> impl IntoResponse {
        let method = req.method().to_string();
        let path = req.uri().path().to_string();
        let authorization = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
            .await
            .unwrap_or_default();
        let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
        state
            .captured
            .lock()
            .unwrap()
            .push(CapturedDiscordOutbound {
                method,
                path,
                authorization,
                body,
            });
        let ok = state.status.is_success();
        (state.status, Json(json!({ "ok": ok })))
    }

    async fn spawn_discord_api_mock(
        status: StatusCode,
    ) -> (String, Arc<Mutex<Vec<CapturedDiscordOutbound>>>) {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let state = DiscordApiMockState {
            status,
            captured: captured.clone(),
        };
        let app = Router::new()
            .fallback(discord_api_mock_fallback)
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind discord mock");
        let addr = listener.local_addr().expect("discord mock local_addr");
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("discord mock serve");
        });
        (format!("http://{addr}"), captured)
    }

    fn create_test_app_with_full(
        api_token: Option<String>,
        webhook_secret: Option<String>,
        rate_limit_per_minute: Option<u32>,
        session_ttl_secs: Option<u64>,
    ) -> Router {
        create_test_app_with_metrics(
            api_token,
            webhook_secret,
            rate_limit_per_minute,
            session_ttl_secs,
            false,
        )
    }

    fn create_test_app_with_metrics_auth(
        api_token: Option<String>,
        metrics_require_auth: bool,
    ) -> Router {
        create_test_app_with_metrics(api_token, None, None, None, metrics_require_auth)
    }

    fn create_test_app_with_metrics(
        api_token: Option<String>,
        webhook_secret: Option<String>,
        rate_limit_per_minute: Option<u32>,
        session_ttl_secs: Option<u64>,
        metrics_require_auth: bool,
    ) -> Router {
        let config = AgentConfig::default();
        let metrics = Arc::new(Metrics::default());
        let agent = attach_tool_metrics(
            JiaClawAgent::new(config).expect("创建测试 agent 失败"),
            &metrics,
        );
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));
        let state = AppState {
            agent: Arc::new(agent),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            api_token,
            webhook_secret,
            telegram_secret: None,
            telegram_bot_token: None,
            telegram_api_base: TELEGRAM_API_BASE.to_string(),
            slack_signing_secret: None,
            slack_bot_token: None,
            slack_api_base: SLACK_API_BASE.to_string(),
            discord_public_key: None,
            discord_bot_token: None,
            discord_api_base: DISCORD_API_BASE.to_string(),
            persist_enabled: false,
            persist_path: Arc::new(persist_path),
            rate_limiter: rate_limit_per_minute.and_then(build_rate_limiter),
            session_ttl: session_ttl_secs.map(Duration::from_secs),
            metrics,
            metrics_require_auth,
        };

        build_router(state)
    }

    fn create_test_state_from_config(config: AgentConfig) -> AppState {
        let agent = JiaClawAgent::new(config).expect("创建测试 agent 失败");
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));
        AppState {
            agent: Arc::new(agent),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            api_token: None,
            webhook_secret: None,
            telegram_secret: None,
            telegram_bot_token: None,
            telegram_api_base: TELEGRAM_API_BASE.to_string(),
            slack_signing_secret: None,
            slack_bot_token: None,
            slack_api_base: SLACK_API_BASE.to_string(),
            discord_public_key: None,
            discord_bot_token: None,
            discord_api_base: DISCORD_API_BASE.to_string(),
            persist_enabled: false,
            persist_path: Arc::new(persist_path),
            rate_limiter: None,
            session_ttl: None,
            metrics: Arc::new(Metrics::default()),
            metrics_require_auth: false,
        }
    }

    fn overflow_history(user_count: usize) -> Vec<ChatMessage> {
        let mut messages = vec![ChatMessage {
            role: MessageRole::System,
            content: "你是一个助手".to_string(),
        }];
        for i in 0..user_count {
            messages.push(ChatMessage {
                role: MessageRole::User,
                content: format!("消息 {i}"),
            });
        }
        messages
    }

    async fn http_create_session(app: &Router) -> String {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: CreateSessionResponse = serde_json::from_slice(&body).unwrap();
        created.session_id
    }

    async fn http_get_session_status(app: &Router, session_id: &str) -> StatusCode {
        app.clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/sessions/{session_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }

    async fn http_list_session_ids(app: &Router) -> Vec<String> {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let list: ListSessionsResponse = serde_json::from_slice(&body).unwrap();
        list.sessions.into_iter().map(|s| s.id).collect()
    }

    async fn http_delete_session(app: &Router, session_id: &str) -> DeleteSessionResponse {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/sessions/{session_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    async fn http_chat_with_session(app: &Router, session_id: &str) {
        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "ttl-touch".to_string(),
            }],
            enabled_tools: vec![],
            enabled_skills: vec![],
            auto_skills: true,
            session_id: Some(session_id.to_string()),
        };
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let health: HealthResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(health.status, "ok");
        assert_eq!(health.agent_name, "JiaClaw");
        assert!(!health.version.is_empty());
    }

    #[tokio::test]
    async fn test_chat_endpoint() {
        let app = create_test_app();

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

        let request_body = serde_json::to_string(&request).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let chat_response: ChatResponse = serde_json::from_slice(&body).unwrap();

        assert!(!chat_response.message.content.is_empty());
    }

    #[tokio::test]
    async fn test_chat_endpoint_with_tool_call() {
        let app = create_test_app();

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

        let request_body = serde_json::to_string(&request).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let chat_response: ChatResponse = serde_json::from_slice(&body).unwrap();

        // 在存根模式下，应该触发工具调用
        assert!(!chat_response.tool_calls.is_empty(), "应该有工具调用");
        assert_eq!(chat_response.tool_calls[0].tool_name, "workspace_list");
    }

    #[tokio::test]
    async fn test_create_session() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let create_response: CreateSessionResponse = serde_json::from_slice(&body).unwrap();

        assert!(!create_response.session_id.is_empty());
    }

    #[tokio::test]
    async fn test_chat_with_session() {
        let app = create_test_app();

        // 第一次请求，创建 session
        let session_id = "test-session-123".to_string();
        let request1 = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "你好".to_string(),
            }],
            enabled_tools: vec![],
            enabled_skills: vec![],
            auto_skills: true,
            session_id: Some(session_id.clone()),
        };

        let request_body1 = serde_json::to_string(&request1).unwrap();

        let response1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body1))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response1.status(), StatusCode::OK);

        let body1 = axum::body::to_bytes(response1.into_body(), usize::MAX)
            .await
            .unwrap();
        let chat_response1: ChatResponse = serde_json::from_slice(&body1).unwrap();

        assert_eq!(chat_response1.session_id, Some(session_id.clone()));
        assert!(!chat_response1.message.content.is_empty());

        // 第二次请求，使用同一个 session
        let request2 = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "我是谁？".to_string(),
            }],
            enabled_tools: vec![],
            enabled_skills: vec![],
            auto_skills: true,
            session_id: Some(session_id.clone()),
        };

        let request_body2 = serde_json::to_string(&request2).unwrap();

        let response2 = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body2))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response2.status(), StatusCode::OK);

        let body2 = axum::body::to_bytes(response2.into_body(), usize::MAX)
            .await
            .unwrap();
        let chat_response2: ChatResponse = serde_json::from_slice(&body2).unwrap();

        assert_eq!(chat_response2.session_id, Some(session_id));
        assert!(!chat_response2.message.content.is_empty());
    }

    #[tokio::test]
    async fn test_delete_session() {
        let app = create_test_app();

        let session_id = "test-session-to-delete".to_string();

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/sessions/{}", session_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let delete_response: DeleteSessionResponse = serde_json::from_slice(&body).unwrap();

        // Session 不存在时也应该返回 200，但 success 为 false
        assert!(!delete_response.success);
    }

    #[tokio::test]
    async fn test_create_then_delete_session_success() {
        let app = create_test_app();

        // 创建 session
        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(create_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let create_result: CreateSessionResponse = serde_json::from_slice(&body).unwrap();
        let session_id = create_result.session_id;

        // 删除刚创建的 session
        let delete_response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/sessions/{}", session_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(delete_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(delete_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let delete_result: DeleteSessionResponse = serde_json::from_slice(&body).unwrap();

        // 应该成功删除
        assert!(delete_result.success, "刚创建的 session 应该能成功删除");
        assert!(delete_result.message.contains(&session_id));
    }

    #[tokio::test]
    async fn test_list_sessions_empty() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().get(X_REQUEST_ID).is_some());

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let list: ListSessionsResponse = serde_json::from_slice(&body).unwrap();
        assert!(list.sessions.is_empty());
    }

    #[tokio::test]
    async fn test_create_then_list_and_get_session() {
        let app = create_test_app();

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: CreateSessionResponse = serde_json::from_slice(&body).unwrap();
        let session_id = created.session_id;

        let list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let list: ListSessionsResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(list.sessions.len(), 1);
        assert_eq!(list.sessions[0].id, session_id);
        assert_eq!(list.sessions[0].message_count, 0);

        let get_response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/sessions/{session_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(get_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let got: GetSessionResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(got.id, session_id);
        assert!(got.messages.is_empty());
    }

    #[tokio::test]
    async fn test_get_session_returns_messages_after_chat() {
        let app = create_test_app();
        let session_id = "query-session-chat".to_string();

        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "你好".to_string(),
            }],
            enabled_tools: vec![],
            enabled_skills: vec![],
            auto_skills: true,
            session_id: Some(session_id.clone()),
        };

        let chat_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(chat_response.status(), StatusCode::OK);

        let get_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/sessions/{session_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(get_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let got: GetSessionResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(got.id, session_id);
        assert!(
            got.messages.len() >= 2,
            "chat 后应包含用户消息与助手回复，实际: {}",
            got.messages.len()
        );
        assert_eq!(got.messages[0].role, MessageRole::User);
        assert_eq!(got.messages[0].content, "你好");
        assert_eq!(got.messages.last().unwrap().role, MessageRole::Assistant);

        let list_response = app
            .oneshot(
                Request::builder()
                    .uri("/api/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let list: ListSessionsResponse = serde_json::from_slice(&body).unwrap();
        let summary = list
            .sessions
            .iter()
            .find(|s| s.id == session_id)
            .expect("list 应包含刚聊过的 session");
        assert_eq!(summary.message_count, got.messages.len());
    }

    fn parse_jsonl_messages(body: &str) -> Vec<ChatMessage> {
        body.lines()
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_str(line).expect("每行应为 ChatMessage JSON"))
            .collect()
    }

    async fn http_export_session(
        app: &Router,
        session_id: &str,
        format: Option<&str>,
    ) -> axum::http::Response<Body> {
        let uri = match format {
            Some(fmt) => format!("/api/sessions/{session_id}/export?format={fmt}"),
            None => format!("/api/sessions/{session_id}/export"),
        };
        app.clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_export_session_jsonl_line_count_matches_chat() {
        let app = create_test_app();
        let session_id = "export-session-chat".to_string();

        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "你好".to_string(),
            }],
            enabled_tools: vec![],
            enabled_skills: vec![],
            auto_skills: true,
            session_id: Some(session_id.clone()),
        };

        let chat_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(chat_response.status(), StatusCode::OK);

        let get_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/sessions/{session_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(get_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let got: GetSessionResponse = serde_json::from_slice(&body).unwrap();

        let export_response = http_export_session(&app, &session_id, None).await;
        assert_eq!(export_response.status(), StatusCode::OK);
        assert!(!request_id_header(&export_response).is_empty());
        assert_eq!(
            response_content_type(&export_response),
            SESSION_EXPORT_NDJSON
        );
        let export_body = body_text(export_response).await;
        let exported = parse_jsonl_messages(&export_body);
        assert_eq!(exported.len(), got.messages.len());
        assert_eq!(exported, got.messages);
    }

    #[tokio::test]
    async fn test_export_session_json_format() {
        let app = create_test_app();
        let session_id = "export-session-json".to_string();

        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "导出 JSON".to_string(),
            }],
            enabled_tools: vec![],
            enabled_skills: vec![],
            auto_skills: true,
            session_id: Some(session_id.clone()),
        };
        let chat_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(chat_response.status(), StatusCode::OK);

        let export_response = http_export_session(&app, &session_id, Some("json")).await;
        assert_eq!(export_response.status(), StatusCode::OK);
        assert!(response_content_type(&export_response).starts_with("application/json"));
        let body = axum::body::to_bytes(export_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let exported: GetSessionResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(exported.id, session_id);
        assert!(
            exported.messages.len() >= 2,
            "json 导出应包含用户与助手消息，实际: {}",
            exported.messages.len()
        );
        assert_eq!(exported.messages[0].role, MessageRole::User);
        assert_eq!(exported.messages[0].content, "导出 JSON");
    }

    #[tokio::test]
    async fn test_export_session_not_found() {
        let app = create_test_app();
        let response = http_export_session(&app, "does-not-exist", None).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(response.headers().get(X_REQUEST_ID).is_some());
        let payload: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(payload["error"], "session_not_found");
    }

    #[tokio::test]
    async fn test_export_session_does_not_compact_or_rewrite() {
        let state = create_test_state_from_config(AgentConfig::default());
        let session_id = "export-no-compact".to_string();
        let history = overflow_history(60);
        let original_len = history.len();
        {
            let mut sessions = state.sessions.lock().unwrap();
            sessions.insert(session_id.clone(), SessionRecord::new(history));
        }
        let app = build_router(state.clone());

        let export_response = http_export_session(&app, &session_id, None).await;
        assert_eq!(export_response.status(), StatusCode::OK);
        let exported = parse_jsonl_messages(&body_text(export_response).await);
        assert_eq!(exported.len(), original_len);

        let stored_len = {
            let sessions = state.sessions.lock().unwrap();
            sessions
                .get(&session_id)
                .expect("export 不应删除 session")
                .messages
                .len()
        };
        assert_eq!(stored_len, original_len, "导出不应触发摘要或改写 store");
    }

    #[tokio::test(start_paused = true)]
    async fn test_export_session_ttl_expired_is_not_found() {
        let app = create_test_app_with_session_ttl(Some(1));
        let session_id = http_create_session(&app).await;

        let ok = http_export_session(&app, &session_id, None).await;
        assert_eq!(ok.status(), StatusCode::OK);

        tokio::time::advance(Duration::from_secs(1)).await;

        let expired = http_export_session(&app, &session_id, None).await;
        assert_eq!(expired.status(), StatusCode::NOT_FOUND);
        let payload: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(expired.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(payload["error"], "session_not_found");
    }

    #[tokio::test(start_paused = true)]
    async fn test_export_session_does_not_refresh_ttl() {
        let app = create_test_app_with_session_ttl(Some(1));
        let session_id = http_create_session(&app).await;

        tokio::time::advance(Duration::from_millis(700)).await;
        let export_response = http_export_session(&app, &session_id, None).await;
        assert_eq!(export_response.status(), StatusCode::OK);

        tokio::time::advance(Duration::from_millis(700)).await;
        assert_eq!(
            http_get_session_status(&app, &session_id).await,
            StatusCode::NOT_FOUND,
            "export 不应刷新 last_accessed"
        );
    }

    #[tokio::test]
    async fn test_get_session_not_found() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/sessions/does-not-exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(response.headers().get(X_REQUEST_ID).is_some());
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error"], "session_not_found");
    }

    #[tokio::test]
    async fn test_get_session_404_after_delete() {
        let app = create_test_app();

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: CreateSessionResponse = serde_json::from_slice(&body).unwrap();
        let session_id = created.session_id;

        let delete_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/sessions/{session_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(delete_response.status(), StatusCode::OK);

        let get_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/sessions/{session_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_response.status(), StatusCode::NOT_FOUND);

        let list_response = app
            .oneshot(
                Request::builder()
                    .uri("/api/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = axum::body::to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let list: ListSessionsResponse = serde_json::from_slice(&body).unwrap();
        assert!(!list.sessions.iter().any(|s| s.id == session_id));
    }

    #[tokio::test(start_paused = true)]
    async fn test_session_ttl_expires_after_idle() {
        let app = create_test_app_with_session_ttl(Some(1));
        let session_id = http_create_session(&app).await;

        assert_eq!(
            http_get_session_status(&app, &session_id).await,
            StatusCode::OK
        );

        tokio::time::advance(Duration::from_secs(1)).await;

        assert_eq!(
            http_get_session_status(&app, &session_id).await,
            StatusCode::NOT_FOUND
        );
        assert!(!http_list_session_ids(&app).await.contains(&session_id));

        let deleted = http_delete_session(&app, &session_id).await;
        assert!(!deleted.success, "过期 session 的 DELETE 应与不存在一致");
    }

    #[tokio::test(start_paused = true)]
    async fn test_session_ttl_disabled_does_not_expire() {
        let app = create_test_app_with_session_ttl(None);
        let session_id = http_create_session(&app).await;

        tokio::time::advance(Duration::from_secs(10)).await;

        assert_eq!(
            http_get_session_status(&app, &session_id).await,
            StatusCode::OK
        );
        assert!(http_list_session_ids(&app).await.contains(&session_id));
    }

    #[tokio::test(start_paused = true)]
    async fn test_session_ttl_get_touch_refreshes() {
        let app = create_test_app_with_session_ttl(Some(1));
        let session_id = http_create_session(&app).await;

        tokio::time::advance(Duration::from_millis(700)).await;
        assert_eq!(
            http_get_session_status(&app, &session_id).await,
            StatusCode::OK
        );

        tokio::time::advance(Duration::from_millis(700)).await;
        assert_eq!(
            http_get_session_status(&app, &session_id).await,
            StatusCode::OK,
            "GET 触达应刷新 last_accessed"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_session_ttl_chat_touch_refreshes() {
        let app = create_test_app_with_session_ttl(Some(1));
        let session_id = http_create_session(&app).await;

        tokio::time::advance(Duration::from_millis(700)).await;
        http_chat_with_session(&app, &session_id).await;

        tokio::time::advance(Duration::from_millis(700)).await;
        assert_eq!(
            http_get_session_status(&app, &session_id).await,
            StatusCode::OK,
            "chat 触达应刷新 last_accessed"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_session_ttl_purge_saves_when_persist_enabled() {
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));
        let session_id = "ttl-persist".to_string();
        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: "将过期".to_string(),
        }];

        let config = AgentConfig::default();
        let agent = JiaClawAgent::new(config).expect("创建测试 agent 失败");
        let state = AppState {
            agent: Arc::new(agent),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            api_token: None,
            webhook_secret: None,
            telegram_secret: None,
            telegram_bot_token: None,
            telegram_api_base: TELEGRAM_API_BASE.to_string(),
            slack_signing_secret: None,
            slack_bot_token: None,
            slack_api_base: SLACK_API_BASE.to_string(),
            discord_public_key: None,
            discord_bot_token: None,
            discord_api_base: DISCORD_API_BASE.to_string(),
            persist_enabled: true,
            persist_path: Arc::new(persist_path.clone()),
            rate_limiter: None,
            session_ttl: Some(Duration::from_secs(1)),
            metrics: Arc::new(Metrics::default()),
            metrics_require_auth: false,
        };

        {
            let mut map = state.sessions.lock().unwrap();
            map.insert(session_id.clone(), SessionRecord::new(messages));
            persist_session_map(&state, &map);
        }
        assert_eq!(load_sessions(&persist_path).len(), 1);

        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(purge_expired_sessions(&state), 1);
        assert!(
            load_sessions(&persist_path).is_empty(),
            "过期清理后落盘应同步删除"
        );

        std::fs::remove_file(&persist_path).ok();
    }

    fn unique_workspace(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn test_state_for_workspace(workspace: PathBuf) -> AppState {
        let config = AgentConfig {
            workspace_path: workspace,
            ..AgentConfig::default()
        };
        let agent = JiaClawAgent::new(config).expect("创建测试 agent 失败");
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));
        AppState {
            agent: Arc::new(agent),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            api_token: None,
            webhook_secret: None,
            telegram_secret: None,
            telegram_bot_token: None,
            telegram_api_base: TELEGRAM_API_BASE.to_string(),
            slack_signing_secret: None,
            slack_bot_token: None,
            slack_api_base: SLACK_API_BASE.to_string(),
            discord_public_key: None,
            discord_bot_token: None,
            discord_api_base: DISCORD_API_BASE.to_string(),
            persist_enabled: false,
            persist_path: Arc::new(persist_path),
            rate_limiter: None,
            session_ttl: None,
            metrics: Arc::new(Metrics::default()),
            metrics_require_auth: false,
        }
    }

    fn heartbeat_config(enabled: bool, interval_secs: u64, path: &str) -> AgentConfig {
        AgentConfig {
            workspace_path: unique_workspace("jiaclaw-hb-cfg"),
            heartbeat: HeartbeatConfig {
                enabled,
                interval_secs,
                path: path.to_string(),
                ..HeartbeatConfig::default()
            },
            ..AgentConfig::default()
        }
    }

    struct CountdownLatch {
        remaining: std::sync::atomic::AtomicUsize,
        notify: tokio::sync::Notify,
    }

    impl CountdownLatch {
        fn new(count: usize) -> Arc<Self> {
            Arc::new(Self {
                remaining: std::sync::atomic::AtomicUsize::new(count),
                notify: tokio::sync::Notify::new(),
            })
        }

        fn count_down(&self) {
            let prev = self
                .remaining
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            if prev == 1 {
                self.notify.notify_waiters();
            }
        }

        async fn wait(&self) {
            loop {
                if self.remaining.load(std::sync::atomic::Ordering::SeqCst) == 0 {
                    return;
                }
                let notified = self.notify.notified();
                if self.remaining.load(std::sync::atomic::Ordering::SeqCst) == 0 {
                    return;
                }
                notified.await;
            }
        }
    }

    #[test]
    fn heartbeat_disabled_does_not_spawn() {
        let config = heartbeat_config(false, 1, "HEARTBEAT.md");
        let state = test_state_for_workspace(config.workspace_path.clone());
        assert!(
            maybe_spawn_heartbeat(state, &config, None).is_none(),
            "enabled=false 不得启动心跳任务"
        );
        let _ = std::fs::remove_dir_all(&config.workspace_path);
    }

    #[tokio::test]
    async fn heartbeat_empty_file_skips_without_session() {
        let ws = unique_workspace("jiaclaw-hb-empty");
        std::fs::write(ws.join("HEARTBEAT.md"), "  \n").unwrap();
        let state = test_state_for_workspace(ws.clone());
        let outcome = run_heartbeat_tick(&state, &ws, "HEARTBEAT.md", "heartbeat").await;
        assert_eq!(outcome, HeartbeatTickOutcome::SkippedEmptyOrMissing);
        assert!(
            state.sessions.lock().unwrap().is_empty(),
            "空文件跳过时不应写入 session"
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[tokio::test]
    async fn heartbeat_missing_file_skips_without_session() {
        let ws = unique_workspace("jiaclaw-hb-missing");
        let state = test_state_for_workspace(ws.clone());
        let outcome = run_heartbeat_tick(&state, &ws, "HEARTBEAT.md", "heartbeat").await;
        assert_eq!(outcome, HeartbeatTickOutcome::SkippedEmptyOrMissing);
        assert!(state.sessions.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[tokio::test(start_paused = true)]
    async fn heartbeat_short_interval_fires_at_least_once() {
        let ws = unique_workspace("jiaclaw-hb-fire");
        std::fs::write(ws.join("HEARTBEAT.md"), "periodic self-check: stretch").unwrap();
        let config = AgentConfig {
            workspace_path: ws.clone(),
            heartbeat: HeartbeatConfig {
                enabled: true,
                interval_secs: 1,
                path: "HEARTBEAT.md".to_string(),
                session_id: "heartbeat".to_string(),
            },
            ..AgentConfig::default()
        };

        let state = test_state_for_workspace(ws.clone());
        let latch = CountdownLatch::new(1);
        let latch_hook = latch.clone();
        let handle = maybe_spawn_heartbeat(
            state.clone(),
            &config,
            Some(Arc::new(move || latch_hook.count_down())),
        )
        .expect("enabled=true 应启动心跳任务");

        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(1)).await;
        latch.wait().await;

        let sessions = state.sessions.lock().unwrap();
        let rec = sessions.get("heartbeat").expect("心跳应写入固定 session");
        assert!(rec
            .messages
            .iter()
            .any(|m| m.role == MessageRole::User && m.content.contains("periodic self-check")));
        assert!(rec
            .messages
            .iter()
            .any(|m| m.role == MessageRole::Assistant));
        drop(sessions);

        handle.abort();
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[tokio::test]
    async fn heartbeat_tick_writes_user_and_assistant_to_session() {
        let ws = unique_workspace("jiaclaw-hb-once");
        std::fs::write(ws.join("HEARTBEAT.md"), "remind me to drink water").unwrap();
        let state = test_state_for_workspace(ws.clone());
        let outcome = run_heartbeat_tick(&state, &ws, "HEARTBEAT.md", "heartbeat").await;
        match outcome {
            HeartbeatTickOutcome::Completed { user_chars, .. } => {
                assert!(user_chars > 0);
            }
            other => panic!("expected completed tick, got {other:?}"),
        }
        let sessions = state.sessions.lock().unwrap();
        let rec = sessions.get("heartbeat").expect("session");
        assert_eq!(rec.messages[0].role, MessageRole::User);
        assert!(rec.messages[0].content.contains("drink water"));
        assert!(rec
            .messages
            .iter()
            .any(|m| m.role == MessageRole::Assistant));
        drop(sessions);
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[tokio::test]
    async fn test_get_session_reads_memory_not_disk() {
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));
        let session_id = "mem-session".to_string();
        let mem_messages = vec![ChatMessage {
            role: MessageRole::User,
            content: "内存中的消息".to_string(),
        }];
        let disk_messages = vec![ChatMessage {
            role: MessageRole::User,
            content: "磁盘上的旧消息".to_string(),
        }];

        let mut disk_map = HashMap::new();
        disk_map.insert(session_id.clone(), disk_messages);
        save_sessions(&persist_path, &disk_map).expect("保存失败");

        let config = AgentConfig::default();
        let agent = JiaClawAgent::new(config).expect("创建测试 agent 失败");
        let mut mem_map = HashMap::new();
        mem_map.insert(session_id.clone(), SessionRecord::new(mem_messages));
        let state = AppState {
            agent: Arc::new(agent),
            sessions: Arc::new(Mutex::new(mem_map)),
            api_token: None,
            webhook_secret: None,
            telegram_secret: None,
            telegram_bot_token: None,
            telegram_api_base: TELEGRAM_API_BASE.to_string(),
            slack_signing_secret: None,
            slack_bot_token: None,
            slack_api_base: SLACK_API_BASE.to_string(),
            discord_public_key: None,
            discord_bot_token: None,
            discord_api_base: DISCORD_API_BASE.to_string(),
            persist_enabled: true,
            persist_path: Arc::new(persist_path.clone()),
            rate_limiter: None,
            session_ttl: None,
            metrics: Arc::new(Metrics::default()),
            metrics_require_auth: false,
        };
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/sessions/{session_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let got: GetSessionResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(got.messages.len(), 1);
        assert_eq!(got.messages[0].content, "内存中的消息");

        std::fs::remove_file(&persist_path).ok();
    }

    #[tokio::test]
    async fn test_session_message_limit() {
        let state = create_test_state_from_config(AgentConfig::default());
        let session_id = "test-limit-session".to_string();

        {
            let mut sessions = state.sessions.lock().unwrap();
            sessions.insert(session_id.clone(), SessionRecord::new(overflow_history(60)));
        }

        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "新消息".to_string(),
            }],
            enabled_tools: vec![],
            enabled_skills: vec![],
            auto_skills: true,
            session_id: Some(session_id.clone()),
        };

        let app = Router::new()
            .route("/api/chat", post(chat_handler))
            .with_state(state.clone());

        let request_body = serde_json::to_string(&request).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let chat_response: ChatResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(chat_response.session_id, Some(session_id.clone()));

        let stored = {
            let sessions = state.sessions.lock().unwrap();
            sessions
                .get(&session_id)
                .expect("session 应存在")
                .messages
                .clone()
        };

        // 硬截断后写入 assistant，可能略超上限
        assert!(
            stored.len() <= MAX_SESSION_MESSAGES + 1,
            "截断后应接近上限，实际 {}",
            stored.len()
        );
        assert_eq!(stored[0].role, MessageRole::System);
        assert_eq!(stored[0].content, "你是一个助手");
        assert!(stored.iter().any(|m| m.content == "新消息"));
        assert!(
            !stored
                .iter()
                .any(|m| m.role == MessageRole::User && m.content == "消息 0"),
            "最旧用户消息应被丢弃"
        );
        assert!(!stored[0].content.contains(SESSION_SUMMARY_PREFIX));
    }

    #[tokio::test]
    async fn test_session_summarize_on_overflow_replaces_old_messages() {
        let config = AgentConfig {
            session: SessionConfig {
                summarize_on_overflow: true,
                keep_recent: 10,
            },
            ..AgentConfig::default()
        };
        let state = create_test_state_from_config(config);
        let session_id = "test-summarize-session".to_string();

        {
            let mut sessions = state.sessions.lock().unwrap();
            sessions.insert(session_id.clone(), SessionRecord::new(overflow_history(60)));
        }

        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "新消息".to_string(),
            }],
            enabled_tools: vec![],
            enabled_skills: vec![],
            auto_skills: true,
            session_id: Some(session_id.clone()),
        };

        let app = Router::new()
            .route("/api/chat", post(chat_handler))
            .with_state(state.clone());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let stored = {
            let sessions = state.sessions.lock().unwrap();
            sessions
                .get(&session_id)
                .expect("session 应存在")
                .messages
                .clone()
        };

        assert_eq!(stored[0].role, MessageRole::System);
        assert!(
            stored[0].content.contains(SESSION_SUMMARY_PREFIX),
            "旧消息应折叠为带标记的摘要: {}",
            stored[0].content
        );
        assert!(stored.iter().any(|m| m.content == "新消息"));
        assert!(stored
            .iter()
            .any(|m| m.role == MessageRole::User && m.content == "消息 59"));
        assert!(
            !stored
                .iter()
                .any(|m| m.role == MessageRole::User && m.content == "消息 0"),
            "被摘要的旧用户消息不应再作为独立条目"
        );
        assert!(
            stored.len() < 20,
            "摘要压缩后历史应远小于硬截断上限，实际 {}",
            stored.len()
        );
    }

    #[tokio::test]
    async fn test_webhook_session_summarize_on_overflow() {
        let config = AgentConfig {
            session: SessionConfig {
                summarize_on_overflow: true,
                keep_recent: 10,
            },
            ..AgentConfig::default()
        };
        let state = create_test_state_from_config(config);
        let session_id = "webhook:overflow-chat".to_string();
        {
            let mut sessions = state.sessions.lock().unwrap();
            sessions.insert(session_id.clone(), SessionRecord::new(overflow_history(60)));
        }

        let app = build_router(state.clone());
        let webhook_request = InboundWebhookRequest {
            channel: "webhook".to_string(),
            chat_id: "overflow-chat".to_string(),
            text: "新消息".to_string(),
            username: None,
        };
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/inbound")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&webhook_request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let stored = {
            let sessions = state.sessions.lock().unwrap();
            sessions
                .get(&session_id)
                .expect("webhook session 应存在")
                .messages
                .clone()
        };
        assert!(stored[0].content.contains(SESSION_SUMMARY_PREFIX));
        assert!(stored.iter().any(|m| m.content == "新消息"));
        assert!(!stored
            .iter()
            .any(|m| m.role == MessageRole::User && m.content == "消息 0"));
    }

    #[tokio::test]
    async fn test_tools_endpoint() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/tools")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let tools_response: ToolsResponse = serde_json::from_slice(&body).unwrap();

        // 应该有一些已注册的工具
        assert!(!tools_response.tools.is_empty(), "应该有已注册的工具");
        assert!(
            tools_response
                .tools
                .iter()
                .any(|t| t.name == "memory_append"),
            "应注册 memory_append 工具"
        );
        assert!(
            tools_response.tools.iter().any(|t| t.name == "web_search"),
            "应注册默认 web_search 工具"
        );
        assert!(
            tools_response.tools.iter().any(|t| t.name == "web_fetch"),
            "应注册默认 web_fetch 工具"
        );
        assert!(
            tools_response
                .tools
                .iter()
                .any(|t| t.name == "memory_search"),
            "应注册默认 memory_search 工具"
        );

        // 验证工具信息包含名称和描述
        for tool in &tools_response.tools {
            assert!(!tool.name.is_empty(), "工具名称不应为空");
            assert!(!tool.description.is_empty(), "工具描述不应为空");
        }
    }

    #[tokio::test]
    async fn test_tools_endpoint_omits_web_search_when_disabled() {
        let config = AgentConfig {
            workspace_path: unique_workspace("jiaclaw-web-search-off"),
            tools: ToolsConfig {
                web_search: WebSearchToolConfig {
                    enabled: false,
                    brave_api_key: None,
                },
                ..ToolsConfig::default()
            },
            ..AgentConfig::default()
        };
        let agent = JiaClawAgent::new(config).expect("创建测试 agent 失败");
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));
        let state = AppState {
            agent: Arc::new(agent),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            api_token: None,
            webhook_secret: None,
            telegram_secret: None,
            telegram_bot_token: None,
            telegram_api_base: TELEGRAM_API_BASE.to_string(),
            slack_signing_secret: None,
            slack_bot_token: None,
            slack_api_base: SLACK_API_BASE.to_string(),
            discord_public_key: None,
            discord_bot_token: None,
            discord_api_base: DISCORD_API_BASE.to_string(),
            persist_enabled: false,
            persist_path: Arc::new(persist_path),
            rate_limiter: None,
            session_ttl: None,
            metrics: Arc::new(Metrics::default()),
            metrics_require_auth: false,
        };
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/tools")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let tools_response: ToolsResponse = serde_json::from_slice(&body).unwrap();
        assert!(
            tools_response.tools.iter().all(|t| t.name != "web_search"),
            "enabled=false 时不应出现 web_search"
        );
    }

    #[tokio::test]
    async fn test_tools_endpoint_omits_web_fetch_when_disabled() {
        let config = AgentConfig {
            workspace_path: unique_workspace("jiaclaw-web-fetch-off"),
            tools: ToolsConfig {
                web_fetch: WebFetchToolConfig {
                    enabled: false,
                    allow_private: false,
                },
                ..ToolsConfig::default()
            },
            ..AgentConfig::default()
        };
        let agent = JiaClawAgent::new(config).expect("创建测试 agent 失败");
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));
        let state = AppState {
            agent: Arc::new(agent),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            api_token: None,
            webhook_secret: None,
            telegram_secret: None,
            telegram_bot_token: None,
            telegram_api_base: TELEGRAM_API_BASE.to_string(),
            slack_signing_secret: None,
            slack_bot_token: None,
            slack_api_base: SLACK_API_BASE.to_string(),
            discord_public_key: None,
            discord_bot_token: None,
            discord_api_base: DISCORD_API_BASE.to_string(),
            persist_enabled: false,
            persist_path: Arc::new(persist_path),
            rate_limiter: None,
            session_ttl: None,
            metrics: Arc::new(Metrics::default()),
            metrics_require_auth: false,
        };
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/tools")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let tools_response: ToolsResponse = serde_json::from_slice(&body).unwrap();
        assert!(
            tools_response.tools.iter().all(|t| t.name != "web_fetch"),
            "enabled=false 时不应出现 web_fetch"
        );
        assert!(
            tools_response.tools.iter().any(|t| t.name == "web_search"),
            "关闭 web_fetch 不应影响 web_search"
        );
    }

    #[tokio::test]
    async fn test_tools_endpoint_omits_memory_search_when_disabled() {
        let config = AgentConfig {
            workspace_path: unique_workspace("jiaclaw-memory-search-off"),
            tools: ToolsConfig {
                memory_search: MemorySearchToolConfig { enabled: false },
                ..ToolsConfig::default()
            },
            ..AgentConfig::default()
        };
        let agent = JiaClawAgent::new(config).expect("创建测试 agent 失败");
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));
        let state = AppState {
            agent: Arc::new(agent),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            api_token: None,
            webhook_secret: None,
            telegram_secret: None,
            telegram_bot_token: None,
            telegram_api_base: TELEGRAM_API_BASE.to_string(),
            slack_signing_secret: None,
            slack_bot_token: None,
            slack_api_base: SLACK_API_BASE.to_string(),
            discord_public_key: None,
            discord_bot_token: None,
            discord_api_base: DISCORD_API_BASE.to_string(),
            persist_enabled: false,
            persist_path: Arc::new(persist_path),
            rate_limiter: None,
            session_ttl: None,
            metrics: Arc::new(Metrics::default()),
            metrics_require_auth: false,
        };
        let app = build_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/tools")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let tools_response: ToolsResponse = serde_json::from_slice(&body).unwrap();
        assert!(
            tools_response
                .tools
                .iter()
                .all(|t| t.name != "memory_search"),
            "enabled=false 时不应出现 memory_search"
        );
        assert!(
            tools_response.tools.iter().any(|t| t.name == "web_search"),
            "关闭 memory_search 不应影响 web_search"
        );
        assert!(
            tools_response.tools.iter().any(|t| t.name == "web_fetch"),
            "关闭 memory_search 不应影响 web_fetch"
        );
    }

    #[test]
    fn web_search_status_lines_do_not_print_api_key() {
        let config = AgentConfig {
            tools: ToolsConfig {
                web_search: WebSearchToolConfig {
                    enabled: true,
                    brave_api_key: Some("BSA-super-secret-key".to_string()),
                },
                ..ToolsConfig::default()
            },
            ..AgentConfig::default()
        };
        let joined = web_search_status_lines(&config).join("\n");
        assert!(
            !joined.contains("BSA-super-secret-key"),
            "doctor must not print api key: {joined}"
        );
        assert!(joined.contains("已配置"), "{joined}");
    }

    #[test]
    fn web_fetch_status_line_reports_enabled_and_private_policy() {
        let enabled = AgentConfig::default();
        assert!(web_fetch_status_line(&enabled).contains("已启用"));
        assert!(web_fetch_status_line(&enabled).contains("拒绝"));

        let disabled = AgentConfig {
            tools: ToolsConfig {
                web_fetch: WebFetchToolConfig {
                    enabled: false,
                    allow_private: false,
                },
                ..ToolsConfig::default()
            },
            ..AgentConfig::default()
        };
        assert!(web_fetch_status_line(&disabled).contains("已关闭"));

        let allow_private = AgentConfig {
            tools: ToolsConfig {
                web_fetch: WebFetchToolConfig {
                    enabled: true,
                    allow_private: true,
                },
                ..ToolsConfig::default()
            },
            ..AgentConfig::default()
        };
        assert!(web_fetch_status_line(&allow_private).contains("allow_private"));
    }

    #[test]
    fn memory_search_status_line_reports_enabled_and_disabled() {
        let enabled = AgentConfig::default();
        assert!(memory_search_status_line(&enabled).contains("已启用"));
        assert!(memory_search_status_line(&enabled).contains("MEMORY"));

        let disabled = AgentConfig {
            tools: ToolsConfig {
                memory_search: MemorySearchToolConfig { enabled: false },
                ..ToolsConfig::default()
            },
            ..AgentConfig::default()
        };
        assert!(memory_search_status_line(&disabled).contains("已关闭"));
    }

    #[test]
    fn max_tool_iterations_status_line_shows_effective_default() {
        let config = AgentConfig::default();
        let line = max_tool_iterations_status_line(&config);
        assert!(
            line.contains(&config.effective_max_tool_iterations().to_string()),
            "{line}"
        );
        assert!(
            line.contains("配置文件") || line.contains("环境变量"),
            "{line}"
        );
    }

    #[tokio::test]
    async fn test_skills_endpoint() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/skills")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let skills_response: SkillsResponse = serde_json::from_slice(&body).unwrap();

        // 技能可能为空（如果工作空间不存在或没有技能）
        // 只验证响应格式正确
        for skill in &skills_response.skills {
            assert!(!skill.name.is_empty(), "技能名称不应为空");
            assert!(!skill.description.is_empty(), "技能描述不应为空");
            assert!(!skill.path.is_empty(), "技能路径不应为空");
        }
    }

    #[tokio::test]
    async fn test_webhook_inbound_without_secret() {
        let app = create_test_app();

        let webhook_request = InboundWebhookRequest {
            channel: "webhook".to_string(),
            chat_id: "test-chat-123".to_string(),
            text: "你好，这是一个测试消息".to_string(),
            username: Some("test_user".to_string()),
        };

        let request_body = serde_json::to_string(&webhook_request).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/inbound")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let webhook_response: InboundWebhookResponse = serde_json::from_slice(&body).unwrap();

        assert!(webhook_response.ok);
        assert_eq!(webhook_response.session_id, "webhook:test-chat-123");
        assert!(webhook_response.reply.is_some());
        assert!(webhook_response.message.is_some());
    }

    #[tokio::test]
    async fn test_webhook_inbound_with_secret_missing_header() {
        let app = create_test_app_with_secret(Some("test-secret-123".to_string()));

        let webhook_request = InboundWebhookRequest {
            channel: "webhook".to_string(),
            chat_id: "test-chat-456".to_string(),
            text: "测试消息".to_string(),
            username: None,
        };

        let request_body = serde_json::to_string(&webhook_request).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/inbound")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(error_response["ok"], false);
        assert_eq!(error_response["error"], "unauthorized");
    }

    #[tokio::test]
    async fn test_webhook_inbound_with_secret_wrong_secret() {
        let app = create_test_app_with_secret(Some("correct-secret".to_string()));

        let webhook_request = InboundWebhookRequest {
            channel: "webhook".to_string(),
            chat_id: "test-chat-789".to_string(),
            text: "测试消息".to_string(),
            username: None,
        };

        let request_body = serde_json::to_string(&webhook_request).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/inbound")
                    .header("content-type", "application/json")
                    .header("X-Webhook-Secret", "wrong-secret")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error_response: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(error_response["ok"], false);
        assert_eq!(error_response["error"], "unauthorized");
    }

    #[tokio::test]
    async fn test_webhook_inbound_with_secret_correct() {
        let app = create_test_app_with_secret(Some("correct-secret".to_string()));

        let webhook_request = InboundWebhookRequest {
            channel: "webhook".to_string(),
            chat_id: "test-chat-correct".to_string(),
            text: "认证成功的消息".to_string(),
            username: Some("authenticated_user".to_string()),
        };

        let request_body = serde_json::to_string(&webhook_request).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/inbound")
                    .header("content-type", "application/json")
                    .header("X-Webhook-Secret", "correct-secret")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let webhook_response: InboundWebhookResponse = serde_json::from_slice(&body).unwrap();

        assert!(webhook_response.ok);
        assert_eq!(webhook_response.session_id, "webhook:test-chat-correct");
        assert!(webhook_response.reply.is_some());
        assert!(webhook_response.message.is_some());
    }

    #[tokio::test]
    async fn test_webhook_inbound_session_persistence() {
        let app = create_test_app();

        // 第一次调用
        let webhook_request1 = InboundWebhookRequest {
            channel: "webhook".to_string(),
            chat_id: "persistent-chat".to_string(),
            text: "第一条消息".to_string(),
            username: None,
        };

        let request_body1 = serde_json::to_string(&webhook_request1).unwrap();

        let response1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/inbound")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body1))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response1.status(), StatusCode::OK);

        // 第二次调用，使用相同的 chat_id
        let webhook_request2 = InboundWebhookRequest {
            channel: "webhook".to_string(),
            chat_id: "persistent-chat".to_string(),
            text: "第二条消息".to_string(),
            username: None,
        };

        let request_body2 = serde_json::to_string(&webhook_request2).unwrap();

        let response2 = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/inbound")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body2))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response2.status(), StatusCode::OK);

        let body2 = axum::body::to_bytes(response2.into_body(), usize::MAX)
            .await
            .unwrap();
        let webhook_response2: InboundWebhookResponse = serde_json::from_slice(&body2).unwrap();

        assert!(webhook_response2.ok);
        assert_eq!(webhook_response2.session_id, "webhook:persistent-chat");
    }

    fn telegram_text_update(chat_id: i64, text: &str) -> String {
        serde_json::json!({
            "update_id": 1001,
            "message": {
                "message_id": 10,
                "chat": { "id": chat_id, "type": "private" },
                "text": text
            }
        })
        .to_string()
    }

    async fn post_telegram(
        app: &Router,
        body: String,
        secret: Option<&str>,
    ) -> axum::http::Response<Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/hooks/telegram")
            .header("content-type", "application/json");
        if let Some(token) = secret {
            builder = builder.header(X_TELEGRAM_BOT_API_SECRET_TOKEN, token);
        }
        app.clone()
            .oneshot(builder.body(Body::from(body)).unwrap())
            .await
            .unwrap()
    }

    #[test]
    fn test_telegram_inbound_text_from_message_and_edited() {
        let message_update: TelegramUpdate = serde_json::from_value(serde_json::json!({
            "update_id": 1,
            "message": {
                "message_id": 1,
                "chat": { "id": 4242 },
                "text": "  hello  "
            }
        }))
        .unwrap();
        assert_eq!(
            telegram_inbound_text(&message_update),
            Some(("4242".to_string(), "hello".to_string()))
        );

        let edited: TelegramUpdate = serde_json::from_value(serde_json::json!({
            "update_id": 2,
            "edited_message": {
                "message_id": 2,
                "chat": { "id": -100_123 },
                "text": "edited"
            }
        }))
        .unwrap();
        assert_eq!(
            telegram_inbound_text(&edited),
            Some(("-100123".to_string(), "edited".to_string()))
        );

        let no_text: TelegramUpdate = serde_json::from_value(serde_json::json!({
            "update_id": 3,
            "message": {
                "message_id": 3,
                "chat": { "id": 1 },
                "photo": []
            }
        }))
        .unwrap();
        assert_eq!(telegram_inbound_text(&no_text), None);

        let string_id: TelegramUpdate = serde_json::from_value(serde_json::json!({
            "update_id": 4,
            "message": {
                "chat": { "id": "abc" },
                "text": "hi"
            }
        }))
        .unwrap();
        assert_eq!(
            telegram_inbound_text(&string_id),
            Some(("abc".to_string(), "hi".to_string()))
        );
    }

    #[tokio::test]
    async fn test_telegram_inbound_creates_and_reuses_session() {
        let app = create_test_app();
        let client_id = "tg-req-create";

        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/telegram")
                    .header("content-type", "application/json")
                    .header("X-Request-Id", client_id)
                    .body(Body::from(telegram_text_update(4242, "你好 Telegram")))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(first.status(), StatusCode::OK);
        assert_eq!(request_id_header(&first), client_id);

        let body = axum::body::to_bytes(first.into_body(), usize::MAX)
            .await
            .unwrap();
        let tg: TelegramInboundResponse = serde_json::from_slice(&body).unwrap();
        assert!(tg.ok);
        assert!(tg.skipped.is_none());
        assert_eq!(tg.session_id.as_deref(), Some("telegram:4242"));
        assert!(tg.reply.as_ref().is_some_and(|r| !r.is_empty()));

        let got = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/sessions/telegram:4242")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(got.status(), StatusCode::OK);
        let session_body = axum::body::to_bytes(got.into_body(), usize::MAX)
            .await
            .unwrap();
        let session: GetSessionResponse = serde_json::from_slice(&session_body).unwrap();
        let first_count = session.messages.len();
        assert!(first_count >= 2, "应包含 user + assistant");

        let second = post_telegram(&app, telegram_text_update(4242, "第二句"), None).await;
        assert_eq!(second.status(), StatusCode::OK);
        let body2 = axum::body::to_bytes(second.into_body(), usize::MAX)
            .await
            .unwrap();
        let tg2: TelegramInboundResponse = serde_json::from_slice(&body2).unwrap();
        assert_eq!(tg2.session_id.as_deref(), Some("telegram:4242"));

        let got2 = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/sessions/telegram:4242")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let session_body2 = axum::body::to_bytes(got2.into_body(), usize::MAX)
            .await
            .unwrap();
        let session2: GetSessionResponse = serde_json::from_slice(&session_body2).unwrap();
        assert!(
            session2.messages.len() > first_count,
            "复用 session 时应追加消息"
        );

        let inbound_still_works = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/inbound")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "channel": "webhook",
                            "chat_id": "user123",
                            "text": "inbound 仍可用"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(inbound_still_works.status(), StatusCode::OK);
        let inbound_body = axum::body::to_bytes(inbound_still_works.into_body(), usize::MAX)
            .await
            .unwrap();
        let inbound: InboundWebhookResponse = serde_json::from_slice(&inbound_body).unwrap();
        assert!(inbound.ok);
        assert_eq!(inbound.session_id, "webhook:user123");
    }

    #[tokio::test]
    async fn test_telegram_skips_update_without_text() {
        let app = create_test_app();
        let body = serde_json::json!({
            "update_id": 99,
            "message": {
                "message_id": 1,
                "chat": { "id": 777, "type": "private" },
                "photo": [{ "file_id": "aaa" }]
            }
        })
        .to_string();

        let response = post_telegram(&app, body, None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(!request_id_header(&response).is_empty());

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let tg: TelegramInboundResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(tg.ok);
        assert_eq!(tg.skipped, Some(true));
        assert_eq!(tg.reason.as_deref(), Some("no text in update"));
        assert!(tg.reply.is_none());

        let missing = http_get_session_status(&app, "telegram:777").await;
        assert_eq!(missing, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_telegram_edited_message_text() {
        let app = create_test_app();
        let body = serde_json::json!({
            "update_id": 12,
            "edited_message": {
                "message_id": 3,
                "chat": { "id": 888 },
                "text": "编辑后的文本"
            }
        })
        .to_string();

        let response = post_telegram(&app, body, None).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let tg: TelegramInboundResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(tg.ok);
        assert_eq!(tg.session_id.as_deref(), Some("telegram:888"));
        assert!(tg.reply.is_some());
    }

    #[tokio::test]
    async fn test_telegram_secret_missing_header() {
        let app = create_test_app_with_telegram_secret(Some("tg-secret".to_string()));
        let response = post_telegram(&app, telegram_text_update(1, "hi"), None).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let err: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(err["ok"], false);
        assert_eq!(err["error"], "unauthorized");
    }

    #[tokio::test]
    async fn test_telegram_secret_wrong_token() {
        let app = create_test_app_with_telegram_secret(Some("tg-secret".to_string()));
        let response = post_telegram(&app, telegram_text_update(1, "hi"), Some("wrong")).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_telegram_secret_correct() {
        let app = create_test_app_with_telegram_secret(Some("tg-secret".to_string()));
        let response = post_telegram(&app, telegram_text_update(55, "ok"), Some("tg-secret")).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let tg: TelegramInboundResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(tg.ok);
        assert_eq!(tg.session_id.as_deref(), Some("telegram:55"));
    }

    #[tokio::test]
    async fn test_telegram_secret_does_not_affect_inbound() {
        let app = create_test_app_with_telegram_secret(Some("tg-secret".to_string()));
        let inbound = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/inbound")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "chat_id": "still-open",
                            "text": "webhook 不走 telegram secret"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(inbound.status(), StatusCode::OK);
    }

    #[test]
    fn test_truncate_telegram_text_at_4096_chars() {
        let exact = "a".repeat(TELEGRAM_MAX_TEXT_LEN);
        assert_eq!(truncate_telegram_text(&exact), exact);
        let over: String = "你".repeat(TELEGRAM_MAX_TEXT_LEN + 8);
        let truncated = truncate_telegram_text(&over);
        assert_eq!(truncated.chars().count(), TELEGRAM_MAX_TEXT_LEN);
        assert!(truncated.chars().all(|c| c == '你'));
    }

    #[tokio::test]
    async fn test_telegram_without_bot_token_does_not_call_send_message() {
        let (base, captured) = spawn_telegram_api_mock(StatusCode::OK).await;
        let app = create_test_app_with_telegram_outbound(None, base);
        let response = post_telegram(&app, telegram_text_update(4242, "你好"), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let tg: TelegramInboundResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(tg.ok);
        assert!(tg.reply.as_ref().is_some_and(|r| !r.is_empty()));
        assert!(tg.delivered.is_none());
        assert!(tg.delivery_error.is_none());
        let raw: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(raw.get("delivered").is_none());
        assert!(raw.get("delivery_error").is_none());
        assert!(captured.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_telegram_send_message_called_with_chat_id_and_text() {
        let token = "123456:TEST-TOKEN";
        let (base, captured) = spawn_telegram_api_mock(StatusCode::OK).await;
        let app = create_test_app_with_telegram_outbound(Some(token.to_string()), base);
        let response = post_telegram(&app, telegram_text_update(4242, "你好 Telegram"), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let tg: TelegramInboundResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(tg.ok);
        let reply = tg.reply.expect("reply");
        assert!(!reply.is_empty());
        assert_eq!(tg.delivered, Some(true));
        assert!(tg.delivery_error.is_none());

        let captured = captured.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].method, "POST");
        assert_eq!(captured[0].path, format!("/bot{token}/sendMessage"));
        assert_eq!(json_chat_id(&captured[0].body).as_deref(), Some("4242"));
        assert_eq!(captured[0].body["text"].as_str(), Some(reply.as_str()));
    }

    #[tokio::test]
    async fn test_telegram_send_message_5xx_still_returns_200_with_reply() {
        let token = "123456:TEST-TOKEN";
        let (base, captured) = spawn_telegram_api_mock(StatusCode::INTERNAL_SERVER_ERROR).await;
        let app = create_test_app_with_telegram_outbound(Some(token.to_string()), base);
        let response = post_telegram(&app, telegram_text_update(99, "hello"), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let tg: TelegramInboundResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(tg.ok);
        assert!(tg.reply.as_ref().is_some_and(|r| !r.is_empty()));
        assert_eq!(tg.delivered, Some(false));
        assert!(
            tg.delivery_error
                .as_ref()
                .is_some_and(|err| err.contains("500")),
            "delivery_error={:?}",
            tg.delivery_error
        );
        assert_eq!(captured.lock().unwrap().len(), 1);
    }

    fn slack_url_verification(challenge: &str) -> String {
        json!({
            "type": "url_verification",
            "challenge": challenge
        })
        .to_string()
    }

    fn slack_message_event(team_id: Option<&str>, channel: &str, text: &str) -> String {
        let mut value = json!({
            "type": "event_callback",
            "event": {
                "type": "message",
                "channel": channel,
                "user": "U123",
                "text": text
            }
        });
        if let Some(team) = team_id {
            value["team_id"] = json!(team);
        }
        value.to_string()
    }

    fn slack_subtype_event(subtype: &str) -> String {
        json!({
            "type": "event_callback",
            "team_id": "TTEAM",
            "event": {
                "type": "message",
                "subtype": subtype,
                "channel": "CCHAN",
                "text": "should skip",
                "bot_id": "B123"
            }
        })
        .to_string()
    }

    async fn post_slack(
        app: &Router,
        body: String,
        signature: Option<(&str, &str)>,
    ) -> axum::http::Response<Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/hooks/slack")
            .header("content-type", "application/json");
        if let Some((timestamp, sig)) = signature {
            builder = builder
                .header(X_SLACK_REQUEST_TIMESTAMP, timestamp)
                .header(X_SLACK_SIGNATURE, sig);
        }
        app.clone()
            .oneshot(builder.body(Body::from(body)).unwrap())
            .await
            .unwrap()
    }

    async fn post_slack_signed(
        app: &Router,
        body: String,
        secret: &str,
    ) -> axum::http::Response<Body> {
        let timestamp = unix_now_secs().to_string();
        let signature = slack_v0_signature(secret, &timestamp, body.as_bytes()).expect("sign");
        post_slack(app, body, Some((&timestamp, &signature))).await
    }

    #[test]
    fn test_slack_known_hmac_vector() {
        const SECRET: &str = "8f742231b10e8888abcd99yyyzzz85a5";
        const TIMESTAMP: &str = "1531420618";
        const BODY: &[u8] = br#"{"type":"url_verification","challenge":"3eZbrw1aBm2rZgRNFdxV2595E9CY3gmdALWMmHkvFXO7tYXAYM8P"}"#;
        const EXPECTED: &str =
            "v0=2b617e8d1eba789c3e90a54b772adadfc772137987c87d3c1e4129005632e406";
        assert_eq!(
            slack_v0_signature(SECRET, TIMESTAMP, BODY).as_deref(),
            Some(EXPECTED)
        );
        assert!(verify_slack_v0_signature(SECRET, TIMESTAMP, BODY, EXPECTED));
        assert!(!verify_slack_v0_signature(
            SECRET,
            TIMESTAMP,
            BODY,
            "v0=0000000000000000000000000000000000000000000000000000000000000000"
        ));
        assert!(verify_slack_request(
            SECRET,
            Some(TIMESTAMP),
            Some(EXPECTED),
            BODY,
            1_531_420_618
        ));
        assert!(!verify_slack_request(
            SECRET,
            Some(TIMESTAMP),
            Some(EXPECTED),
            BODY,
            1_531_420_618 + 301
        ));
    }

    #[test]
    fn test_slack_timestamp_freshness_window() {
        assert!(slack_timestamp_fresh("1000", 1000));
        assert!(slack_timestamp_fresh("1300", 1000));
        assert!(!slack_timestamp_fresh("1301", 1000));
        assert!(slack_timestamp_fresh("700", 1000));
        assert!(!slack_timestamp_fresh("699", 1000));
        assert!(!slack_timestamp_fresh("nope", 1000));
        assert!(!slack_timestamp_fresh("-1", 1000));
    }

    #[test]
    fn test_classify_slack_envelope_message_and_skips() {
        let verification: SlackEnvelope =
            serde_json::from_str(&slack_url_verification("abc")).unwrap();
        assert_eq!(
            classify_slack_envelope(&verification),
            SlackInboundKind::UrlVerification {
                challenge: "abc".to_string()
            }
        );

        let with_team: SlackEnvelope =
            serde_json::from_str(&slack_message_event(Some("T1"), "C9", "hello")).unwrap();
        assert_eq!(
            classify_slack_envelope(&with_team),
            SlackInboundKind::Message {
                session_id: "slack:T1:C9".to_string(),
                channel: "C9".to_string(),
                text: "hello".to_string(),
            }
        );

        let no_team: SlackEnvelope =
            serde_json::from_str(&slack_message_event(None, "C9", "hello")).unwrap();
        assert_eq!(
            classify_slack_envelope(&no_team),
            SlackInboundKind::Message {
                session_id: "slack:C9".to_string(),
                channel: "C9".to_string(),
                text: "hello".to_string(),
            }
        );

        let bot: SlackEnvelope = serde_json::from_str(&slack_subtype_event("bot_message")).unwrap();
        assert_eq!(
            classify_slack_envelope(&bot),
            SlackInboundKind::Skipped {
                reason: "ignored message subtype".to_string()
            }
        );

        let changed: SlackEnvelope =
            serde_json::from_str(&slack_subtype_event("message_changed")).unwrap();
        assert_eq!(
            classify_slack_envelope(&changed),
            SlackInboundKind::Skipped {
                reason: "ignored message subtype".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_slack_url_verification_returns_challenge() {
        let app = create_test_app();
        let response = post_slack(&app, slack_url_verification("challenge-xyz"), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(!request_id_header(&response).is_empty());
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["challenge"], "challenge-xyz");
        assert!(value.get("ok").is_none());
        assert!(value.get("reply").is_none());
    }

    #[tokio::test]
    async fn test_slack_message_creates_and_reuses_session() {
        let app = create_test_app();
        let body = slack_message_event(Some("TTEAM"), "CCHAN", "你好 Slack");
        let response = post_slack(&app, body, None).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let slack: SlackInboundResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(slack.ok);
        assert!(slack.reply.as_ref().is_some_and(|r| !r.is_empty()));
        assert_eq!(slack.session_id.as_deref(), Some("slack:TTEAM:CCHAN"));
        assert!(slack.skipped.is_none());

        let history = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/sessions/slack:TTEAM:CCHAN")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(history.status(), StatusCode::OK);

        let second = post_slack(
            &app,
            slack_message_event(Some("TTEAM"), "CCHAN", "第二句"),
            None,
        )
        .await;
        assert_eq!(second.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(second.into_body(), usize::MAX)
            .await
            .unwrap();
        let slack2: SlackInboundResponse = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(slack2.session_id.as_deref(), Some("slack:TTEAM:CCHAN"));
    }

    #[tokio::test]
    async fn test_slack_session_without_team_id() {
        let app = create_test_app();
        let response = post_slack(&app, slack_message_event(None, "CNOTEAM", "hi"), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let slack: SlackInboundResponse = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(slack.session_id.as_deref(), Some("slack:CNOTEAM"));
    }

    #[tokio::test]
    async fn test_slack_skips_bot_message_and_message_changed() {
        let app = create_test_app();
        for subtype in ["bot_message", "message_changed"] {
            let response = post_slack(&app, slack_subtype_event(subtype), None).await;
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let slack: SlackInboundResponse = serde_json::from_slice(&bytes).unwrap();
            assert!(slack.ok);
            assert_eq!(slack.skipped, Some(true));
            assert_eq!(slack.reason.as_deref(), Some("ignored message subtype"));
            assert!(slack.reply.is_none());
        }
        let missing = http_get_session_status(&app, "slack:TTEAM:CCHAN").await;
        assert_eq!(missing, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_slack_skips_empty_text() {
        let app = create_test_app();
        let body = json!({
            "type": "event_callback",
            "team_id": "T1",
            "event": { "type": "message", "channel": "C1", "text": "   " }
        })
        .to_string();
        let response = post_slack(&app, body, None).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let slack: SlackInboundResponse = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(slack.skipped, Some(true));
        assert_eq!(slack.reason.as_deref(), Some("no text in event"));
    }

    #[tokio::test]
    async fn test_slack_signature_missing_when_configured() {
        let app = create_test_app_with_slack_signing_secret(Some("signing-secret".to_string()));
        let response = post_slack(&app, slack_message_event(Some("T1"), "C1", "hi"), None).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_slack_signature_wrong() {
        let secret = "signing-secret";
        let app = create_test_app_with_slack_signing_secret(Some(secret.to_string()));
        let body = slack_message_event(Some("T1"), "C1", "hi");
        let ts = unix_now_secs().to_string();
        let response = post_slack(
            &app,
            body,
            Some((
                &ts,
                "v0=0000000000000000000000000000000000000000000000000000000000000000",
            )),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_slack_signature_correct() {
        let secret = "signing-secret";
        let app = create_test_app_with_slack_signing_secret(Some(secret.to_string()));
        let body = slack_message_event(Some("T55"), "C55", "ok");
        let response = post_slack_signed(&app, body, secret).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let slack: SlackInboundResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(slack.ok);
        assert_eq!(slack.session_id.as_deref(), Some("slack:T55:C55"));
    }

    #[tokio::test]
    async fn test_slack_signature_stale_timestamp() {
        let secret = "signing-secret";
        let app = create_test_app_with_slack_signing_secret(Some(secret.to_string()));
        let body = slack_message_event(Some("T1"), "C1", "hi");
        let ts = "1";
        let sig = slack_v0_signature(secret, ts, body.as_bytes()).expect("sign");
        let response = post_slack(&app, body, Some((ts, &sig))).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_slack_signed_url_verification() {
        let secret = "signing-secret";
        let app = create_test_app_with_slack_signing_secret(Some(secret.to_string()));
        let response = post_slack_signed(&app, slack_url_verification("from-slack"), secret).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["challenge"], "from-slack");
    }

    #[tokio::test]
    async fn test_slack_signing_secret_does_not_affect_inbound_or_telegram() {
        let app = create_test_app_with_slack_signing_secret(Some("signing-secret".to_string()));
        let inbound = InboundWebhookRequest {
            channel: "webhook".to_string(),
            chat_id: "no-slack".to_string(),
            text: "webhook 不走 slack 签名".to_string(),
            username: None,
        };
        let webhook = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/inbound")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&inbound).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(webhook.status(), StatusCode::OK);

        let telegram = post_telegram(&app, telegram_text_update(7, "tg"), None).await;
        assert_eq!(telegram.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_slack_without_bot_token_does_not_call_post_message() {
        let (base, captured) = spawn_slack_api_mock(StatusCode::OK).await;
        let app = create_test_app_with_slack_outbound(None, base);
        let response = post_slack(&app, slack_message_event(Some("T1"), "C1", "你好"), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let slack: SlackInboundResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(slack.ok);
        assert!(slack.delivered.is_none());
        let raw: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(raw.get("delivered").is_none());
        assert!(captured.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_slack_chat_post_message_called_with_channel_and_text() {
        let token = "xoxb-test-token";
        let (base, captured) = spawn_slack_api_mock(StatusCode::OK).await;
        let app = create_test_app_with_slack_outbound(Some(token.to_string()), base);
        let response = post_slack(
            &app,
            slack_message_event(Some("T1"), "C42", "你好 Slack"),
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let slack: SlackInboundResponse = serde_json::from_slice(&bytes).unwrap();
        let reply = slack.reply.expect("reply");
        assert_eq!(slack.delivered, Some(true));

        let captured = captured.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].method, "POST");
        assert_eq!(captured[0].path, "/chat.postMessage");
        assert_eq!(captured[0].authorization, format!("Bearer {token}"));
        assert_eq!(json_channel(&captured[0].body).as_deref(), Some("C42"));
        assert_eq!(captured[0].body["text"].as_str(), Some(reply.as_str()));
    }

    #[tokio::test]
    async fn test_slack_chat_post_message_5xx_still_returns_200_with_reply() {
        let token = "xoxb-test-token";
        let (base, captured) = spawn_slack_api_mock(StatusCode::INTERNAL_SERVER_ERROR).await;
        let app = create_test_app_with_slack_outbound(Some(token.to_string()), base);
        let response = post_slack(&app, slack_message_event(Some("T1"), "C9", "hello"), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let slack: SlackInboundResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(slack.ok);
        assert!(slack.reply.as_ref().is_some_and(|r| !r.is_empty()));
        assert_eq!(slack.delivered, Some(false));
        assert!(
            slack
                .delivery_error
                .as_ref()
                .is_some_and(|err| err.contains("500")),
            "delivery_error={:?}",
            slack.delivery_error
        );
        assert_eq!(captured.lock().unwrap().len(), 1);
    }

    #[test]
    fn test_truncate_slack_text_at_40000_chars() {
        let exact: String = "a".repeat(SLACK_MAX_TEXT_LEN);
        assert_eq!(truncate_slack_text(&exact), exact);
        let over: String = "你".repeat(SLACK_MAX_TEXT_LEN + 8);
        let truncated = truncate_slack_text(&over);
        assert_eq!(truncated.chars().count(), SLACK_MAX_TEXT_LEN);
        assert!(truncated.chars().all(|c| c == '你'));
    }

    fn discord_test_keypair() -> (String, ring::signature::Ed25519KeyPair) {
        use ring::signature::KeyPair;
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng)
            .expect("generate discord test key");
        let pair = ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
            .expect("parse discord test key");
        let public_hex = encode_hex_lower(pair.public_key().as_ref());
        (public_hex, pair)
    }

    fn sign_discord_body(
        signing_key: &ring::signature::Ed25519KeyPair,
        timestamp: &str,
        body: &[u8],
    ) -> String {
        let mut message = Vec::with_capacity(timestamp.len() + body.len());
        message.extend_from_slice(timestamp.as_bytes());
        message.extend_from_slice(body);
        encode_hex_lower(signing_key.sign(&message).as_ref())
    }

    fn discord_ping() -> String {
        json!({ "type": 1 }).to_string()
    }

    fn discord_chat_command(
        guild_id: Option<&str>,
        channel_id: &str,
        prompt: &str,
        application_id: &str,
        token: &str,
    ) -> String {
        let mut value = json!({
            "type": 2,
            "application_id": application_id,
            "channel_id": channel_id,
            "token": token,
            "data": {
                "name": "ask",
                "type": 1,
                "options": [{
                    "name": "prompt",
                    "type": 3,
                    "value": prompt
                }]
            }
        });
        if let Some(guild) = guild_id {
            value["guild_id"] = json!(guild);
        }
        value.to_string()
    }

    async fn post_discord(
        app: &Router,
        body: String,
        signature: Option<(&str, &str)>,
    ) -> axum::http::Response<Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/hooks/discord")
            .header("content-type", "application/json");
        if let Some((timestamp, sig)) = signature {
            builder = builder
                .header(X_SIGNATURE_TIMESTAMP, timestamp)
                .header(X_SIGNATURE_ED25519, sig);
        }
        app.clone()
            .oneshot(builder.body(Body::from(body)).unwrap())
            .await
            .unwrap()
    }

    async fn post_discord_signed(
        app: &Router,
        body: String,
        signing_key: &ring::signature::Ed25519KeyPair,
    ) -> axum::http::Response<Body> {
        let timestamp = unix_now_secs().to_string();
        let signature = sign_discord_body(signing_key, &timestamp, body.as_bytes());
        post_discord(app, body, Some((&timestamp, &signature))).await
    }

    async fn wait_for_session(app: &Router, session_id: &str) {
        for _ in 0..100 {
            if http_get_session_status(app, session_id).await == StatusCode::OK {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("session {session_id} 未在超时内创建");
    }

    async fn wait_captured_discord(captured: &Arc<Mutex<Vec<CapturedDiscordOutbound>>>, n: usize) {
        for _ in 0..100 {
            if captured.lock().unwrap().len() >= n {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("Discord mock 未在超时内收到 {n} 次调用");
    }

    #[test]
    fn test_classify_discord_ping_command_and_skips() {
        let ping: DiscordInteraction = serde_json::from_str(&discord_ping()).unwrap();
        assert_eq!(
            classify_discord_interaction(&ping),
            DiscordInboundKind::Ping
        );

        let with_guild: DiscordInteraction = serde_json::from_str(&discord_chat_command(
            Some("G1"),
            "C9",
            "hello",
            "APP",
            "tok",
        ))
        .unwrap();
        assert_eq!(
            classify_discord_interaction(&with_guild),
            DiscordInboundKind::ChatCommand {
                session_id: "discord:G1:C9".to_string(),
                text: "hello".to_string(),
                application_id: "APP".to_string(),
                interaction_token: "tok".to_string(),
            }
        );

        let dm: DiscordInteraction =
            serde_json::from_str(&discord_chat_command(None, "C9", "hello", "APP", "tok")).unwrap();
        assert_eq!(
            classify_discord_interaction(&dm),
            DiscordInboundKind::ChatCommand {
                session_id: "discord:dm:C9".to_string(),
                text: "hello".to_string(),
                application_id: "APP".to_string(),
                interaction_token: "tok".to_string(),
            }
        );

        let named: DiscordInteraction = serde_json::from_str(
            &json!({
                "type": 2,
                "application_id": "APP",
                "channel_id": "C1",
                "token": "tok",
                "data": { "name": "status", "type": 1 }
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(
            classify_discord_interaction(&named),
            DiscordInboundKind::ChatCommand {
                session_id: "discord:dm:C1".to_string(),
                text: "status".to_string(),
                application_id: "APP".to_string(),
                interaction_token: "tok".to_string(),
            }
        );

        let component: DiscordInteraction =
            serde_json::from_str(&json!({ "type": 3, "channel_id": "C1" }).to_string()).unwrap();
        assert_eq!(
            classify_discord_interaction(&component),
            DiscordInboundKind::Skipped {
                reason: "ignored interaction type".to_string()
            }
        );

        let user_cmd: DiscordInteraction = serde_json::from_str(
            &json!({
                "type": 2,
                "channel_id": "C1",
                "data": { "name": "info", "type": 2 }
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(
            classify_discord_interaction(&user_cmd),
            DiscordInboundKind::Skipped {
                reason: "ignored command type".to_string()
            }
        );
    }

    #[test]
    fn test_discord_signature_roundtrip() {
        let (public_hex, signing_key) = discord_test_keypair();
        let body = br#"{"type":1}"#;
        let timestamp = "1710000000";
        let signature = sign_discord_body(&signing_key, timestamp, body);
        assert!(verify_discord_signature(
            &public_hex,
            timestamp,
            body,
            &signature
        ));
        assert!(!verify_discord_signature(
            &public_hex,
            timestamp,
            body,
            "00"
        ));
        assert!(!verify_discord_request(
            &public_hex,
            None,
            Some(&signature),
            body
        ));
        assert!(!verify_discord_request(
            &public_hex,
            Some(timestamp),
            None,
            body
        ));
    }

    #[tokio::test]
    async fn test_discord_ping_returns_pong() {
        let app = create_test_app();
        let response = post_discord(&app, discord_ping(), None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(!request_id_header(&response).is_empty());
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["type"], 1);
        assert!(value.get("data").is_none());
    }

    #[tokio::test]
    async fn test_discord_command_creates_session_and_deferred_ack() {
        let app = create_test_app();
        let body = discord_chat_command(Some("GTEAM"), "CCHAN", "你好 Discord", "APP", "tok");
        let response = post_discord(&app, body, None).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["type"], 5);

        wait_for_session(&app, "discord:GTEAM:CCHAN").await;
        let history = http_get_session_status(&app, "discord:GTEAM:CCHAN").await;
        assert_eq!(history, StatusCode::OK);

        let second = post_discord(
            &app,
            discord_chat_command(Some("GTEAM"), "CCHAN", "第二句", "APP", "tok2"),
            None,
        )
        .await;
        assert_eq!(second.status(), StatusCode::OK);
        wait_for_session(&app, "discord:GTEAM:CCHAN").await;
    }

    #[tokio::test]
    async fn test_discord_dm_session_without_guild() {
        let app = create_test_app();
        let response = post_discord(
            &app,
            discord_chat_command(None, "CDM", "hi", "APP", "tok"),
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        wait_for_session(&app, "discord:dm:CDM").await;
    }

    #[tokio::test]
    async fn test_discord_signature_missing_when_configured() {
        let (public_hex, _signing_key) = discord_test_keypair();
        let app =
            create_test_app_with_discord(Some(public_hex), None, DISCORD_API_BASE.to_string());
        let response = post_discord(&app, discord_ping(), None).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_discord_signature_wrong() {
        let (public_hex, _signing_key) = discord_test_keypair();
        let app =
            create_test_app_with_discord(Some(public_hex), None, DISCORD_API_BASE.to_string());
        let body = discord_ping();
        let ts = unix_now_secs().to_string();
        let response = post_discord(&app, body, Some((&ts, &"00".repeat(64)))).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_discord_signature_correct_ping_and_command() {
        let (public_hex, signing_key) = discord_test_keypair();
        let app =
            create_test_app_with_discord(Some(public_hex), None, DISCORD_API_BASE.to_string());
        let ping = post_discord_signed(&app, discord_ping(), &signing_key).await;
        assert_eq!(ping.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(ping.into_body(), usize::MAX)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["type"], 1);

        let command = post_discord_signed(
            &app,
            discord_chat_command(Some("G9"), "C9", "signed", "APP", "tok"),
            &signing_key,
        )
        .await;
        assert_eq!(command.status(), StatusCode::OK);
        wait_for_session(&app, "discord:G9:C9").await;
    }

    #[tokio::test]
    async fn test_discord_public_key_does_not_affect_other_hooks() {
        let (public_hex, _signing_key) = discord_test_keypair();
        let app =
            create_test_app_with_discord(Some(public_hex), None, DISCORD_API_BASE.to_string());
        let inbound = InboundWebhookRequest {
            channel: "webhook".to_string(),
            chat_id: "no-discord".to_string(),
            text: "webhook 不走 discord 签名".to_string(),
            username: None,
        };
        let webhook = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/inbound")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&inbound).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(webhook.status(), StatusCode::OK);

        let telegram = post_telegram(&app, telegram_text_update(7, "tg"), None).await;
        assert_eq!(telegram.status(), StatusCode::OK);

        let slack = post_slack(
            &app,
            slack_message_event(Some("T1"), "C1", "slack still works"),
            None,
        )
        .await;
        assert_eq!(slack.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_discord_without_bot_token_records_session_no_followup() {
        let (base, captured) = spawn_discord_api_mock(StatusCode::OK).await;
        let app = create_test_app_with_discord(None, None, base);
        let response = post_discord(
            &app,
            discord_chat_command(Some("G1"), "C1", "你好", "APPID", "tok"),
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        wait_for_session(&app, "discord:G1:C1").await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(captured.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_discord_deferred_followup_patches_original() {
        let token = "discord-bot-token";
        let (base, captured) = spawn_discord_api_mock(StatusCode::OK).await;
        let app = create_test_app_with_discord(None, Some(token.to_string()), base);
        let response = post_discord(
            &app,
            discord_chat_command(Some("G1"), "C42", "你好 Discord", "APPID", "inter-token"),
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["type"], 5);

        wait_captured_discord(&captured, 1).await;
        wait_for_session(&app, "discord:G1:C42").await;
        let captured = captured.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].method, "PATCH");
        assert_eq!(
            captured[0].path,
            "/webhooks/APPID/inter-token/messages/@original"
        );
        assert_eq!(captured[0].authorization, format!("Bot {token}"));
        assert!(captured[0].body["content"]
            .as_str()
            .is_some_and(|text| !text.is_empty()));
    }

    #[tokio::test]
    async fn test_discord_followup_5xx_still_acks_and_keeps_session() {
        let token = "discord-bot-token";
        let (base, captured) = spawn_discord_api_mock(StatusCode::INTERNAL_SERVER_ERROR).await;
        let app = create_test_app_with_discord(None, Some(token.to_string()), base);
        let response = post_discord(
            &app,
            discord_chat_command(Some("G1"), "C9", "hello", "APPID", "tok"),
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        wait_captured_discord(&captured, 1).await;
        wait_for_session(&app, "discord:G1:C9").await;
        assert_eq!(captured.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_session_persistence_disabled() {
        let config = AgentConfig::default();
        let agent = JiaClawAgent::new(config.clone()).expect("创建测试 agent 失败");
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));

        let state = AppState {
            agent: Arc::new(agent),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            api_token: None,
            webhook_secret: None,
            telegram_secret: None,
            telegram_bot_token: None,
            telegram_api_base: TELEGRAM_API_BASE.to_string(),
            slack_signing_secret: None,
            slack_bot_token: None,
            slack_api_base: SLACK_API_BASE.to_string(),
            discord_public_key: None,
            discord_bot_token: None,
            discord_api_base: DISCORD_API_BASE.to_string(),
            persist_enabled: false,
            persist_path: Arc::new(persist_path.clone()),
            rate_limiter: None,
            session_ttl: None,
            metrics: Arc::new(Metrics::default()),
            metrics_require_auth: false,
        };

        let session_id = "test-session".to_string();
        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: "测试消息".to_string(),
        }];

        {
            let mut sessions = state.sessions.lock().unwrap();
            sessions.insert(session_id.clone(), SessionRecord::new(messages.clone()));
        }

        assert!(!persist_path.exists(), "persist=false 时不应该创建文件");
    }

    #[tokio::test]
    async fn test_session_persistence_enabled() {
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));

        let session_id = "test-session".to_string();
        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: "测试消息".to_string(),
        }];

        {
            let mut sessions_map = HashMap::new();
            sessions_map.insert(session_id.clone(), messages.clone());
            save_sessions(&persist_path, &sessions_map).expect("保存失败");
        }

        assert!(persist_path.exists(), "persist=true 时应该创建文件");

        let loaded = load_sessions(&persist_path);
        assert_eq!(loaded.len(), 1, "应该加载 1 个 session");
        assert_eq!(loaded.get(&session_id).unwrap().len(), 1, "应该有 1 条消息");
        assert_eq!(loaded.get(&session_id).unwrap()[0].content, "测试消息");

        std::fs::remove_file(&persist_path).ok();
    }

    #[tokio::test]
    async fn test_session_persistence_corrupted_file() {
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));

        std::fs::write(&persist_path, "{ invalid json ").expect("写入失败");

        let loaded = load_sessions(&persist_path);
        assert_eq!(loaded.len(), 0, "损坏的文件应该返回空 map");

        std::fs::remove_file(&persist_path).ok();
    }

    #[tokio::test]
    async fn test_session_persistence_nonexistent_file() {
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));

        let loaded = load_sessions(&persist_path);
        assert_eq!(loaded.len(), 0, "不存在的文件应该返回空 map");
    }

    #[tokio::test]
    async fn test_session_persistence_delete() {
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));

        let session_id1 = "test-session-1".to_string();
        let session_id2 = "test-session-2".to_string();
        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: "测试消息".to_string(),
        }];

        {
            let mut sessions_map = HashMap::new();
            sessions_map.insert(session_id1.clone(), messages.clone());
            sessions_map.insert(session_id2.clone(), messages.clone());
            save_sessions(&persist_path, &sessions_map).expect("保存失败");
        }

        let mut loaded = load_sessions(&persist_path);
        assert_eq!(loaded.len(), 2, "应该加载 2 个 sessions");

        loaded.remove(&session_id1);
        save_sessions(&persist_path, &loaded).expect("保存失败");

        let loaded_after_delete = load_sessions(&persist_path);
        assert_eq!(loaded_after_delete.len(), 1, "删除后应该剩 1 个 session");
        assert!(loaded_after_delete.contains_key(&session_id2));
        assert!(!loaded_after_delete.contains_key(&session_id1));

        std::fs::remove_file(&persist_path).ok();
    }

    #[tokio::test]
    async fn test_api_auth_not_required_when_no_token() {
        let app = create_test_app();

        // 不带 token 的请求应该成功
        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "测试".to_string(),
            }],
            enabled_tools: vec![],
            enabled_skills: vec![],
            auto_skills: true,
            session_id: None,
        };

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_api_auth_required_with_token() {
        let app = create_test_app_with_auth(Some("test-token-123".to_string()), None);

        // 不带 token 的请求应该返回 401
        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "测试".to_string(),
            }],
            enabled_tools: vec![],
            enabled_skills: vec![],
            auto_skills: true,
            session_id: None,
        };

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_api_auth_bearer_token_success() {
        let app = create_test_app_with_auth(Some("test-token-123".to_string()), None);

        // 带正确 Bearer token 的请求应该成功
        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "测试".to_string(),
            }],
            enabled_tools: vec![],
            enabled_skills: vec![],
            auto_skills: true,
            session_id: None,
        };

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer test-token-123")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_api_auth_x_api_token_success() {
        let app = create_test_app_with_auth(Some("test-token-123".to_string()), None);

        // 带正确 X-Api-Token 的请求应该成功
        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "测试".to_string(),
            }],
            enabled_tools: vec![],
            enabled_skills: vec![],
            auto_skills: true,
            session_id: None,
        };

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .header("x-api-token", "test-token-123")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_api_auth_wrong_token() {
        let app = create_test_app_with_auth(Some("test-token-123".to_string()), None);

        // 带错误 token 的请求应该返回 401
        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "测试".to_string(),
            }],
            enabled_tools: vec![],
            enabled_skills: vec![],
            auto_skills: true,
            session_id: None,
        };

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer wrong-token")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_health_endpoint_no_auth() {
        let app = create_test_app_with_auth(Some("test-token-123".to_string()), None);

        // health 端点不需要鉴权
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn test_sessions_api_requires_auth() {
        let app = create_test_app_with_auth(Some("test-token-123".to_string()), None);

        // POST /api/sessions 应该需要鉴权
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // GET /api/sessions 无 token 应 401
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // GET /api/sessions/:id 无 token 应 401（即使不存在也不应先 404）
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/sessions/any-id")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // GET /api/sessions/:id/export 无 token 应 401（即使不存在也不应先 404）
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/sessions/any-id/export")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // 带正确 token 应该成功
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/sessions")
                    .header("authorization", "Bearer test-token-123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: CreateSessionResponse = serde_json::from_slice(&body).unwrap();
        let session_id = created.session_id;

        let list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/sessions")
                    .header("authorization", "Bearer test-token-123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);

        let get_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/sessions/{session_id}"))
                    .header("authorization", "Bearer test-token-123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_response.status(), StatusCode::OK);

        let export_response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/sessions/{session_id}/export"))
                    .header("authorization", "Bearer test-token-123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(export_response.status(), StatusCode::OK);
        assert_eq!(
            response_content_type(&export_response),
            SESSION_EXPORT_NDJSON
        );
    }

    #[tokio::test]
    async fn test_sessions_query_allows_anonymous_when_no_token() {
        let app = create_test_app();

        let list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);

        let get_response = app
            .oneshot(
                Request::builder()
                    .uri("/api/sessions/missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_tools_api_requires_auth() {
        let app = create_test_app_with_auth(Some("test-token-123".to_string()), None);

        // GET /api/tools 应该需要鉴权
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/tools")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // 带正确 token 应该成功
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/tools")
                    .header("authorization", "Bearer test-token-123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_skills_api_requires_auth() {
        let app = create_test_app_with_auth(Some("test-token-123".to_string()), None);

        // GET /api/skills 应该需要鉴权
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/skills")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // 带正确 token 应该成功
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/skills")
                    .header("authorization", "Bearer test-token-123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_rate_limit_disabled_allows_many_requests() {
        let app = create_test_app();

        for _ in 0..5 {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/api/tools")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn test_rate_limit_returns_429_with_retry_after() {
        let app = create_test_app_with_rate_limit(2);

        let mut statuses = Vec::new();
        for _ in 0..3 {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/api/tools")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            statuses.push(response.status());
            if response.status() == StatusCode::TOO_MANY_REQUESTS {
                let retry_after = response
                    .headers()
                    .get(header::RETRY_AFTER)
                    .expect("429 必须包含 Retry-After")
                    .to_str()
                    .unwrap();
                let secs: u64 = retry_after.parse().expect("Retry-After 应为秒数");
                assert!(secs >= 1, "Retry-After 至少为 1 秒");

                let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap();
                let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(payload["error"], "rate_limit_exceeded");
            }
        }

        assert_eq!(statuses[0], StatusCode::OK);
        assert_eq!(statuses[1], StatusCode::OK);
        assert_eq!(statuses[2], StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn test_health_is_not_rate_limited() {
        let app = create_test_app_with_rate_limit(1);

        // 先打满 /api/* 配额
        let limited = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/tools")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(limited.status(), StatusCode::OK);

        let limited = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/tools")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);

        // GET /health 仍应 200，且 oneshot 不依赖 ConnectInfo
        for _ in 0..3 {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/health")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn test_hooks_inbound_is_rate_limited() {
        let app = create_test_app_with_rate_limit(1);

        let webhook_request = InboundWebhookRequest {
            channel: "webhook".to_string(),
            chat_id: "rate-limit-chat".to_string(),
            text: "限流测试".to_string(),
            username: None,
        };
        let request_body = serde_json::to_string(&webhook_request).unwrap();

        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/inbound")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);

        let second = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/inbound")
                    .header("content-type", "application/json")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(second.headers().get(header::RETRY_AFTER).is_some());
    }

    #[tokio::test]
    async fn test_hooks_telegram_is_rate_limited() {
        let app = create_test_app_with_rate_limit(1);
        let body = telegram_text_update(9, "限流");

        let first = post_telegram(&app, body.clone(), None).await;
        assert_eq!(first.status(), StatusCode::OK);

        let second = post_telegram(&app, body, None).await;
        assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(second.headers().get(header::RETRY_AFTER).is_some());
        assert!(!request_id_header(&second).is_empty());
    }

    #[tokio::test]
    async fn test_hooks_slack_is_rate_limited() {
        let app = create_test_app_with_rate_limit(1);
        let body = slack_message_event(Some("T9"), "C9", "限流");

        let first = post_slack(&app, body.clone(), None).await;
        assert_eq!(first.status(), StatusCode::OK);

        let second = post_slack(&app, body, None).await;
        assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(second.headers().get(header::RETRY_AFTER).is_some());
        assert!(!request_id_header(&second).is_empty());
    }

    #[tokio::test]
    async fn test_hooks_discord_is_rate_limited() {
        let app = create_test_app_with_rate_limit(1);
        let body = discord_chat_command(Some("G9"), "C9", "限流", "APP", "tok");

        let first = post_discord(&app, body.clone(), None).await;
        assert_eq!(first.status(), StatusCode::OK);

        let second = post_discord(&app, body, None).await;
        assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(second.headers().get(header::RETRY_AFTER).is_some());
        assert!(!request_id_header(&second).is_empty());
    }

    #[test]
    fn test_is_rate_limited_path() {
        assert!(is_rate_limited_path("/api/chat"));
        assert!(is_rate_limited_path("/api/tools"));
        assert!(is_rate_limited_path("/api/sessions"));
        assert!(is_rate_limited_path("/api/sessions/abc"));
        assert!(is_rate_limited_path("/api/sessions/abc/export"));
        assert!(is_rate_limited_path("/api/openapi.json"));
        assert!(is_rate_limited_path("/hooks/inbound"));
        assert!(is_rate_limited_path("/hooks/telegram"));
        assert!(is_rate_limited_path("/hooks/slack"));
        assert!(is_rate_limited_path("/hooks/discord"));
        assert!(!is_rate_limited_path("/health"));
        assert!(!is_rate_limited_path("/metrics"));
        assert!(!is_rate_limited_path("/"));
        assert!(!is_rate_limited_path("/api"));
    }

    async fn body_text(response: axum::http::Response<Body>) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).expect("response body utf-8")
    }

    async fn get_metrics(app: &Router) -> (StatusCode, String, Option<String>) {
        get_metrics_with_headers(app, &[]).await
    }

    async fn get_metrics_with_headers(
        app: &Router,
        headers: &[(&str, &str)],
    ) -> (StatusCode, String, Option<String>) {
        let mut builder = Request::builder().uri("/metrics");
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let response = app
            .clone()
            .oneshot(builder.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned);
        let body = body_text(response).await;
        (status, body, content_type)
    }

    #[tokio::test]
    async fn test_metrics_counts_http_requests_and_sessions() {
        let app = create_test_app();

        for _ in 0..2 {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/health")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let created = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::OK);

        let (status, body, content_type) = get_metrics(&app).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(content_type.as_deref(), Some(PROMETHEUS_CONTENT_TYPE));
        assert!(body.contains(
            "jiaclaw_http_requests_total{path=\"health\",method=\"GET\",status=\"200\"} 2"
        ));
        assert!(body.contains(
            "jiaclaw_http_requests_total{path=\"other\",method=\"POST\",status=\"200\"} 1"
        ));
        assert!(body.contains("jiaclaw_sessions_active 1"));
        assert!(body.contains(&format!(
            "jiaclaw_build_info{{version=\"{}\"}} 1",
            env!("CARGO_PKG_VERSION")
        )));
        assert!(
            !body.contains("path=\"metrics\""),
            "当次 scrape 不应计入自身"
        );
    }

    #[tokio::test]
    async fn test_metrics_records_tool_calls() {
        let app = create_test_app();
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
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let (status, body, _) = get_metrics(&app).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains(
            "jiaclaw_http_requests_total{path=\"api_chat\",method=\"POST\",status=\"200\"} 1"
        ));
        assert!(body.contains("jiaclaw_tool_calls_total{tool=\"workspace_list\",result=\"ok\"} 1"));
    }

    #[tokio::test]
    async fn test_metrics_public_even_when_api_token_set() {
        let app = create_test_app_with_auth(Some("test-token-123".to_string()), None);
        let (status, body, content_type) = get_metrics(&app).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(content_type.as_deref(), Some(PROMETHEUS_CONTENT_TYPE));
        assert!(body.contains("jiaclaw_build_info"));
    }

    #[tokio::test]
    async fn test_metrics_requires_auth_when_configured() {
        let app = create_test_app_with_metrics_auth(Some("test-token-123".to_string()), true);

        let (status, body, _) = get_metrics(&app).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(body.contains("Unauthorized"));

        let (status, body, content_type) =
            get_metrics_with_headers(&app, &[("authorization", "Bearer test-token-123")]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(content_type.as_deref(), Some(PROMETHEUS_CONTENT_TYPE));
        assert!(body.contains("jiaclaw_build_info"));

        let health = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_metrics_is_not_rate_limited() {
        let app = create_test_app_with_rate_limit(1);

        let limited = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/tools")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(limited.status(), StatusCode::OK);

        let limited = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/tools")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);

        for _ in 0..3 {
            let (status, _, _) = get_metrics(&app).await;
            assert_eq!(status, StatusCode::OK);
        }

        let health = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);
    }

    fn request_id_header(response: &axum::http::Response<Body>) -> String {
        response
            .headers()
            .get(X_REQUEST_ID)
            .expect("响应必须回写 X-Request-Id")
            .to_str()
            .expect("X-Request-Id 应为 UTF-8")
            .to_string()
    }

    #[tokio::test]
    async fn test_health_generates_request_id() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let request_id = request_id_header(&response);
        assert!(
            uuid::Uuid::parse_str(&request_id).is_ok(),
            "未提供 X-Request-Id 时应生成 UUID，实际: {request_id}"
        );
    }

    #[tokio::test]
    async fn test_request_id_echoed_when_provided() {
        let app = create_test_app();
        let client_id = "client-trace-abc-123";

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .header("X-Request-Id", client_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(request_id_header(&response), client_id);
    }

    #[tokio::test]
    async fn test_chat_echoes_request_id() {
        let app = create_test_app();
        let client_id = "chat-req-001";
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

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .header("X-Request-Id", client_id)
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(request_id_header(&response), client_id);
    }

    #[tokio::test]
    async fn test_rate_limited_response_has_request_id() {
        let app = create_test_app_with_rate_limit(1);
        let client_id = "limited-req-9";

        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/tools")
                    .header("X-Request-Id", client_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);

        let second = app
            .oneshot(
                Request::builder()
                    .uri("/api/tools")
                    .header("X-Request-Id", client_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(request_id_header(&second), client_id);
    }

    #[tokio::test]
    async fn test_openapi_endpoint_returns_paths() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(request_id_header(&response)
            .chars()
            .any(|c| c.is_ascii_hexdigit() || c == '-'));

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let spec: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(spec["openapi"], "3.0.3");
        let paths = spec["paths"].as_object().expect("应包含 paths");
        for required in [
            "/health",
            "/metrics",
            "/api/chat",
            "/api/sessions",
            "/api/sessions/{id}",
            "/api/sessions/{id}/export",
            "/api/tools",
            "/api/skills",
            "/hooks/inbound",
            "/hooks/telegram",
            "/hooks/slack",
            "/hooks/discord",
        ] {
            assert!(
                paths.contains_key(required),
                "OpenAPI paths 缺少 {required}"
            );
        }
        assert!(paths["/api/sessions"].get("get").is_some());
        assert!(paths["/api/sessions"].get("post").is_some());
        assert!(paths["/api/sessions/{id}"].get("get").is_some());
        assert!(paths["/api/sessions/{id}"].get("delete").is_some());
        assert!(paths["/api/sessions/{id}/export"].get("get").is_some());
    }

    #[tokio::test]
    async fn test_openapi_requires_auth_when_token_configured() {
        let app = create_test_app_with_auth(Some("test-token-123".to_string()), None);

        let unauthorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let authorized = app
            .oneshot(
                Request::builder()
                    .uri("/api/openapi.json")
                    .header("authorization", "Bearer test-token-123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authorized.status(), StatusCode::OK);

        let body = axum::body::to_bytes(authorized.into_body(), usize::MAX)
            .await
            .unwrap();
        let spec: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(spec["paths"].is_object());
        assert_eq!(
            spec["components"]["schemas"]["ChatRequest"]["properties"]["stream"]["type"],
            "boolean"
        );
        assert!(
            spec["paths"]["/api/chat"]["post"]["responses"]["200"]["content"]
                .get("text/event-stream")
                .is_some(),
            "OpenAPI 应描述可选 SSE"
        );
    }

    fn response_content_type(response: &axum::http::Response<Body>) -> String {
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    }

    /// 解析 SSE 文本为 `(event, data JSON)`；忽略无 `data:` 的块。
    fn parse_event_stream(body: &str) -> Vec<(String, serde_json::Value)> {
        let mut events = Vec::new();
        for block in body.split("\n\n") {
            let block = block.trim();
            if block.is_empty() {
                continue;
            }
            let mut event_name = "message".to_string();
            let mut data_lines = Vec::new();
            for line in block.lines() {
                if let Some(rest) = line.strip_prefix("event:") {
                    event_name = rest.trim().to_string();
                } else if let Some(rest) = line.strip_prefix("data:") {
                    data_lines.push(rest.trim_start().to_string());
                }
            }
            if data_lines.is_empty() {
                continue;
            }
            let raw = data_lines.join("\n");
            let data = serde_json::from_str(&raw).unwrap_or(json!({ "raw": raw }));
            events.push((event_name, data));
        }
        events
    }

    fn chat_json_body(content: &str, session_id: Option<&str>, stream: bool) -> String {
        let mut value = json!({
            "messages": [{ "role": "user", "content": content }],
            "enabled_tools": [],
            "enabled_skills": [],
            "auto_skills": true,
            "session_id": session_id,
        });
        if stream {
            value["stream"] = json!(true);
        }
        value.to_string()
    }

    #[test]
    fn test_chunk_assistant_text_reconstructs() {
        let text = "你好！我是 JiaClaw。\n这是第二段，用来验证按句分块。";
        let chunks = chunk_assistant_text(text);
        assert!(chunks.len() > 1, "应至少按句切开，实际: {chunks:?}");
        assert_eq!(chunks.concat(), text);
        assert!(chunk_assistant_text("").is_empty());
    }

    #[test]
    fn test_accept_includes_event_stream() {
        let mut headers = HeaderMap::new();
        assert!(!accept_includes_event_stream(&headers));

        headers.insert(header::ACCEPT, HeaderValue::from_static("*/*"));
        assert!(!accept_includes_event_stream(&headers));

        headers.insert(
            header::ACCEPT,
            HeaderValue::from_static("application/json, text/event-stream;q=0.9"),
        );
        assert!(accept_includes_event_stream(&headers));
    }

    #[tokio::test]
    async fn test_chat_json_path_unchanged_without_stream() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(chat_json_body("你好", None, false)))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response_content_type(&response).starts_with("application/json"),
            "未请求流式时应保持 JSON: {}",
            response_content_type(&response)
        );

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let chat_response: ChatResponse = serde_json::from_slice(&body).unwrap();
        assert!(!chat_response.message.content.is_empty());
        assert!(chat_response.session_id.is_none());
    }

    async fn post_chat_sse(
        app: Router,
        body: String,
        extra_headers: &[(&str, &str)],
    ) -> (StatusCode, String, String, String) {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/api/chat")
            .header("content-type", "application/json");
        for (name, value) in extra_headers {
            builder = builder.header(*name, *value);
        }
        let response = app
            .oneshot(builder.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let content_type = response_content_type(&response);
        let request_id = request_id_header(&response);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = String::from_utf8(bytes.to_vec()).expect("SSE 应为 UTF-8");
        (status, content_type, request_id, body)
    }

    #[tokio::test]
    async fn test_chat_sse_accept_header_reaches_done() {
        let app = create_test_app();
        let (status, content_type, request_id, body) = post_chat_sse(
            app,
            chat_json_body("你好", Some("sse-accept"), false),
            &[
                ("accept", "text/event-stream"),
                ("x-request-id", "sse-trace-1"),
            ],
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(
            content_type.starts_with("text/event-stream"),
            "Content-Type 应为 SSE，实际: {content_type}"
        );
        assert_eq!(request_id, "sse-trace-1");

        let events = parse_event_stream(&body);
        let names: Vec<&str> = events.iter().map(|(n, _)| n.as_str()).collect();
        assert!(
            names.contains(&"meta") && names.contains(&"token") && names.contains(&"done"),
            "应包含 meta/token/done，实际: {names:?}\n{body}"
        );
        assert!(
            !names.contains(&"error"),
            "成功路径不应有 error 事件: {body}"
        );

        let meta = events
            .iter()
            .find(|(n, _)| n == "meta")
            .map(|(_, d)| d)
            .expect("缺少 meta");
        assert_eq!(meta["request_id"], "sse-trace-1");
        assert_eq!(meta["session_id"], "sse-accept");

        let done = events
            .iter()
            .find(|(n, _)| n == "done")
            .map(|(_, d)| d)
            .expect("缺少 done");
        assert!(!done["reply"].as_str().unwrap_or("").is_empty());
        assert_eq!(done["session_id"], "sse-accept");
    }

    #[tokio::test]
    async fn test_chat_sse_stream_field_reaches_done_with_tool() {
        let app = create_test_app();
        let (status, content_type, _, body) =
            post_chat_sse(app, chat_json_body("列出工作空间", None, true), &[]).await;

        assert_eq!(status, StatusCode::OK);
        assert!(content_type.starts_with("text/event-stream"));

        let events = parse_event_stream(&body);
        let names: Vec<&str> = events.iter().map(|(n, _)| n.as_str()).collect();
        assert!(
            names.contains(&"done"),
            "应读到 done 事件，实际: {names:?}\n{body}"
        );
        assert!(names.contains(&"tool"), "工具调用应有 tool 事件: {names:?}");

        let tool = events
            .iter()
            .find(|(n, _)| n == "tool")
            .map(|(_, d)| d)
            .expect("缺少 tool");
        assert_eq!(tool["name"], "workspace_list");
        assert_eq!(tool["ok"].as_bool(), Some(true));
    }

    #[tokio::test]
    async fn test_chat_sse_auth_failure_stays_json() {
        let app = create_test_app_with_auth(Some("test-token-123".to_string()), None);
        let (status, content_type, request_id, body) = post_chat_sse(
            app,
            chat_json_body("你好", None, true),
            &[
                ("accept", "text/event-stream"),
                ("x-request-id", "sse-unauth"),
            ],
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(
            content_type.contains("application/json"),
            "鉴权失败应仍为 JSON，实际: {content_type}"
        );
        assert_eq!(request_id, "sse-unauth");
        assert!(
            !body.contains("event:"),
            "鉴权失败不应返回 SSE 事件流: {body}"
        );
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["error"], "Unauthorized");
    }

    #[test]
    fn test_cli_session_export_parses() {
        let cli =
            Cli::try_parse_from(["jiaclaw", "session", "export", "abc-id", "-o", "out.jsonl"])
                .expect("应解析 session export");
        match cli.command {
            Commands::Session {
                action: SessionCommands::Export { id, output, config },
            } => {
                assert_eq!(id, "abc-id");
                assert_eq!(output.as_deref(), Some(std::path::Path::new("out.jsonl")));
                assert!(config.is_none());
            }
            _ => panic!("应为 session export 子命令"),
        }
    }

    #[test]
    fn test_cli_session_export_writes_jsonl_and_404() {
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-export-{}.json", uuid::Uuid::new_v4()));
        let out_path =
            std::env::temp_dir().join(format!("jiaclaw-export-{}.jsonl", uuid::Uuid::new_v4()));
        let session_id = "cli-export-id";
        let mut map = HashMap::new();
        map.insert(
            session_id.to_string(),
            vec![
                ChatMessage {
                    role: MessageRole::User,
                    content: "hi".to_string(),
                },
                ChatMessage {
                    role: MessageRole::Assistant,
                    content: "hello".to_string(),
                },
            ],
        );
        save_sessions(&persist_path, &map).expect("保存 persist 失败");

        export_session_from_persist_file(&persist_path, session_id, Some(&out_path))
            .expect("导出应成功");
        let body = std::fs::read_to_string(&out_path).expect("应写入 jsonl");
        let exported = parse_jsonl_messages(&body);
        assert_eq!(exported.len(), 2);
        assert_eq!(exported[0].content, "hi");
        assert_eq!(exported[1].content, "hello");

        let err = export_session_from_persist_file(&persist_path, "missing", Some(&out_path))
            .expect_err("缺失 session 应失败");
        assert!(
            err.to_string().contains("session_not_found"),
            "错误应为 session_not_found，实际: {err}"
        );

        let before = std::fs::read_to_string(&persist_path).unwrap();
        export_session_from_persist_file(&persist_path, session_id, Some(&out_path)).unwrap();
        let after = std::fs::read_to_string(&persist_path).unwrap();
        assert_eq!(before, after, "CLI 导出不应改写 persist store");

        std::fs::remove_file(&persist_path).ok();
        std::fs::remove_file(&out_path).ok();
    }

    #[test]
    fn test_encode_messages_jsonl_empty() {
        assert_eq!(encode_messages_jsonl(&[]).unwrap(), "");
    }
}
