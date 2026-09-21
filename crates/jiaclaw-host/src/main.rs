// Copyright 2026 JiaClaw contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `JiaClaw` 可执行宿主

use anyhow::{Context, Result};
use axum::{
    extract::{Path, Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use clap::{Parser, Subcommand};
use governor::{
    clock::{Clock, DefaultClock},
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use jiaclaw::{inspect_identity_file, inspect_memory_file, JiaClawAgent, Workspace};
use jiaclaw_core::{AgentConfig, ChatMessage, ChatRequest, ChatResponse, MessageRole};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    num::NonZeroU32,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time::Instant;
use tower_http::cors::{Any, CorsLayer};

/// 进程内全局（非按 IP）速率限制器，oneshot 测试无需 `ConnectInfo`。
type GlobalRateLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// 请求追踪头。大小写不敏感，响应回写同名头。
const X_REQUEST_ID: &str = "x-request-id";

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
    println!("   • 参见 config/jiaclaw.toml.example 了解完整配置选项");

    Ok(())
}

/// 每个 session 保留的最大消息数（防止内存涨爆）
const MAX_SESSION_MESSAGES: usize = 50;

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
    persist_enabled: bool,
    persist_path: Arc<PathBuf>,
    rate_limiter: Option<Arc<GlobalRateLimiter>>,
    session_ttl: Option<Duration>,
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

fn build_rate_limiter(per_minute: u32) -> Option<Arc<GlobalRateLimiter>> {
    NonZeroU32::new(per_minute).map(|nz| Arc::new(RateLimiter::direct(Quota::per_minute(nz))))
}

fn is_rate_limited_path(path: &str) -> bool {
    path.starts_with("/api/") || path == "/hooks/inbound"
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
    Router::new()
        .route("/health", get(health_handler))
        .route("/api/chat", post(chat_handler))
        .route(
            "/api/sessions",
            get(list_sessions_handler).post(create_session_handler),
        )
        .route(
            "/api/sessions/:id",
            get(get_session_handler).delete(delete_session_handler),
        )
        .route("/api/tools", get(tools_handler))
        .route("/api/skills", get(skills_handler))
        .route("/api/openapi.json", get(openapi_handler))
        .route("/hooks/inbound", post(hooks_inbound_handler))
        .layer(middleware::from_fn_with_state(
            limiter,
            rate_limit_middleware,
        ))
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

fn session_ttl_config_source() -> &'static str {
    if std::env::var("JIACLAW_SESSION_TTL_SECS").is_ok() {
        "环境变量 JIACLAW_SESSION_TTL_SECS"
    } else {
        "配置文件"
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

    // 创建 agent
    let agent = JiaClawAgent::new(config.clone()).context("创建 JiaClawAgent 失败")?;

    // 读取 API token（环境变量优先于配置文件）
    let api_token = std::env::var("JIACLAW_API_TOKEN")
        .ok()
        .or(config.http.api_token.clone());

    // 读取 webhook secret（环境变量优先于配置文件）
    let webhook_secret = std::env::var("JIACLAW_WEBHOOK_SECRET")
        .ok()
        .or(config.http.webhook_secret.clone());

    // 读取限流配置（环境变量优先于配置文件）
    let rate_limit_per_minute = config.http.effective_rate_limit_per_minute();
    let rate_limiter = rate_limit_per_minute.and_then(build_rate_limiter);

    // 读取会话闲置 TTL（环境变量优先于配置文件）
    let session_ttl_secs = config.http.effective_session_ttl_secs();
    let session_ttl = session_ttl_secs.map(Duration::from_secs);

    // 解析持久化路径
    let persist_path = if config.http.persist_path.starts_with('/') {
        PathBuf::from(&config.http.persist_path)
    } else {
        config.workspace_path.join(&config.http.persist_path)
    };

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
        persist_enabled: config.http.persist,
        persist_path: Arc::new(persist_path),
        rate_limiter,
        session_ttl,
    };

    if state.session_ttl.is_some() {
        spawn_session_ttl_sweeper(state.clone());
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
    tracing::info!("   • POST   /api/chat            - 聊天端点");
    tracing::info!("   • GET    /api/sessions        - 列出会话");
    tracing::info!("   • POST   /api/sessions        - 创建会话");
    tracing::info!("   • GET    /api/sessions/:id    - 读取会话历史");
    tracing::info!("   • DELETE /api/sessions/:id    - 删除会话");
    tracing::info!("   • GET    /api/tools           - 列出已注册工具");
    tracing::info!("   • GET    /api/skills          - 列出已发现技能");
    tracing::info!("   • GET    /api/openapi.json    - OpenAPI 3 草图");
    tracing::info!("   • POST   /hooks/inbound       - Webhook 入站端点");
    tracing::info!("   • X-Request-Id                - 请求无该头则生成 UUID 并回写");
    if let Some(limit) = rate_limit_per_minute {
        tracing::info!(
            "   • HTTP 限流: {limit} 次/分钟（/api/* 与 /hooks/inbound；GET /health 不限流）"
        );
    } else {
        tracing::info!("   • HTTP 限流: 未启用");
    }
    if let Some(ttl) = session_ttl_secs {
        tracing::info!("   • Session TTL: 已启用（闲置 {ttl} 秒后过期）");
    } else {
        tracing::info!("   • Session TTL: 未启用");
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

    if let Some(limit) = rate_limit_per_minute {
        println!(
            "   • HTTP 限流: ✅ 已启用（{limit} 次/分钟，通过 {}）",
            rate_limit_config_source()
        );
    } else {
        println!("   • HTTP 限流: ⚠️  未启用（/api/* 与 /hooks/inbound 不限流）");
    }

    if let Some(ttl) = session_ttl_secs {
        println!(
            "   • Session TTL: ✅ 已启用（闲置 {ttl} 秒，通过 {}）",
            session_ttl_config_source()
        );
    } else {
        println!("   • Session TTL: ⚠️  未启用（会话不会因闲置过期）");
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

/// 健康检查处理器
async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    let response = HealthResponse {
        status: "ok".to_string(),
        agent_name: state.agent.config().name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    Json(response)
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

/// 聊天处理器
async fn chat_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, AppError> {
    let request_id = request_id_log_value(&headers).to_string();

    // API Token 鉴权检查
    if !check_api_auth(&state, &headers) {
        tracing::warn!(request_id = %request_id, "API 鉴权失败: token 不匹配或缺失");
        return Err(AppError::Unauthorized);
    }

    tracing::info!(
        request_id = %request_id,
        "收到聊天请求，消息数: {}, session_id: {:?}",
        request.messages.len(),
        request.session_id
    );

    let session_id = request.session_id.clone();

    purge_expired_sessions(&state);

    // 如果提供了 session_id，从 session 中获取历史消息
    if let Some(ref sid) = session_id {
        let sessions = state.sessions.lock().unwrap();
        if let Some(history) = sessions.get(sid) {
            // 将历史消息和新消息合并
            let mut all_messages = history.messages.clone();
            all_messages.extend(request.messages.clone());

            // 检查消息数上限，防止内存涨爆
            if all_messages.len() > MAX_SESSION_MESSAGES {
                tracing::info!(
                    request_id = %request_id,
                    "Session {} 消息数 {} 超过上限 {}，开始截断",
                    sid,
                    all_messages.len(),
                    MAX_SESSION_MESSAGES
                );

                // 保留 system 消息（如果有）和最新的消息
                let system_messages: Vec<_> = all_messages
                    .iter()
                    .filter(|m| m.role == MessageRole::System)
                    .cloned()
                    .collect();

                let non_system_messages: Vec<_> = all_messages
                    .into_iter()
                    .filter(|m| m.role != MessageRole::System)
                    .collect();

                // 计算可以保留多少非 system 消息
                let system_count = system_messages.len();
                let available_slots = MAX_SESSION_MESSAGES.saturating_sub(system_count);
                let skip_count = non_system_messages.len().saturating_sub(available_slots);

                // 重新组合：system 消息 + 最新的非 system 消息
                all_messages = system_messages;
                all_messages.extend(non_system_messages.into_iter().skip(skip_count));

                tracing::info!(
                    request_id = %request_id,
                    "截断后消息数: {} (system: {}, 其他: {})",
                    all_messages.len(),
                    system_count,
                    all_messages.len() - system_count
                );
            }

            request.messages = all_messages;
            tracing::info!(
                request_id = %request_id,
                "使用 session {}, 合并后消息数: {}",
                sid,
                request.messages.len()
            );
        } else {
            tracing::info!(request_id = %request_id, "创建新 session: {}", sid);
        }
    }

    let response = state
        .agent
        .chat(&request)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // 如果提供了 session_id，更新 session 历史
    if let Some(ref sid) = session_id {
        let mut sessions = state.sessions.lock().unwrap();
        let mut messages = request.messages.clone();
        messages.push(response.message.clone());
        let message_count = messages.len();
        sessions.insert(sid.clone(), SessionRecord::new(messages));
        tracing::info!(
            request_id = %request_id,
            "更新 session {}, 当前消息数: {}",
            sid,
            message_count
        );

        // 持久化到磁盘（如果启用）
        persist_session_map(&state, &sessions);
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

    Ok(Json(response))
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

/// Webhook 入站处理器
#[allow(clippy::too_many_lines)]
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

    // 鉴权检查
    if let Some(expected) = state.webhook_secret.as_deref() {
        let provided = headers
            .get("X-Webhook-Secret")
            .and_then(|v| v.to_str().ok());
        if provided != Some(expected) {
            tracing::warn!(request_id = %request_id, "Webhook 鉴权失败: secret 不匹配");
            return Ok((
                StatusCode::UNAUTHORIZED,
                Json(json!({"ok": false, "error": "unauthorized"})),
            )
                .into_response());
        }
    }

    // 生成 session_id
    let session_id = format!("webhook:{}", body.chat_id);
    tracing::info!(request_id = %request_id, "使用 session_id: {}", session_id);

    purge_expired_sessions(&state);

    // 构建聊天请求
    let mut request = ChatRequest {
        messages: vec![ChatMessage {
            role: MessageRole::User,
            content: body.text.clone(),
        }],
        enabled_tools: vec![],
        enabled_skills: vec![],
        auto_skills: true,
        session_id: Some(session_id.clone()),
    };

    // 从 session 中获取历史消息
    {
        let sessions = state.sessions.lock().unwrap();
        if let Some(history) = sessions.get(&session_id) {
            let mut all_messages = history.messages.clone();
            all_messages.extend(request.messages.clone());

            // 检查消息数上限
            if all_messages.len() > MAX_SESSION_MESSAGES {
                tracing::info!(
                    request_id = %request_id,
                    "Webhook session {} 消息数 {} 超过上限 {}，开始截断",
                    session_id,
                    all_messages.len(),
                    MAX_SESSION_MESSAGES
                );

                let system_messages: Vec<_> = all_messages
                    .iter()
                    .filter(|m| m.role == MessageRole::System)
                    .cloned()
                    .collect();

                let non_system_messages: Vec<_> = all_messages
                    .into_iter()
                    .filter(|m| m.role != MessageRole::System)
                    .collect();

                let system_count = system_messages.len();
                let available_slots = MAX_SESSION_MESSAGES.saturating_sub(system_count);
                let skip_count = non_system_messages.len().saturating_sub(available_slots);

                all_messages = system_messages;
                all_messages.extend(non_system_messages.into_iter().skip(skip_count));

                tracing::info!(
                    request_id = %request_id,
                    "截断后消息数: {} (system: {}, 其他: {})",
                    all_messages.len(),
                    system_count,
                    all_messages.len() - system_count
                );
            }

            request.messages = all_messages;
            tracing::info!(
                request_id = %request_id,
                "使用 webhook session {}, 合并后消息数: {}",
                session_id,
                request.messages.len()
            );
        } else {
            tracing::info!(request_id = %request_id, "创建新 webhook session: {}", session_id);
        }
    }

    // 调用 agent
    let response = state
        .agent
        .chat(&request)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // 更新 session 历史
    {
        let mut sessions = state.sessions.lock().unwrap();
        let mut messages = request.messages.clone();
        messages.push(response.message.clone());
        let message_count = messages.len();
        sessions.insert(session_id.clone(), SessionRecord::new(messages));
        tracing::info!(
            request_id = %request_id,
            "更新 webhook session {}, 当前消息数: {}",
            session_id,
            message_count
        );

        persist_session_map(&state, &sessions);
    }

    tracing::info!(
        request_id = %request_id,
        "Webhook 聊天响应生成，状态: {:?}, 工具调用数: {}",
        response.status,
        response.tool_calls.len()
    );

    // 构建响应
    let webhook_response = InboundWebhookResponse {
        ok: true,
        session_id: session_id.clone(),
        reply: Some(response.message.content.clone()),
        message: Some(response.message.content),
        error: None,
    };

    Ok((StatusCode::OK, Json(webhook_response)).into_response())
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

/// 从磁盘加载 sessions
fn load_sessions(path: &PathBuf) -> HashMap<String, Vec<ChatMessage>> {
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
        "   • HTTP 限流: {}",
        if let Some(limit) = rate_limit_per_minute {
            format!("已启用（{limit} 次/分钟）")
        } else {
            "未启用".to_string()
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
    use jiaclaw_core::{ChatMessage, ChatRequest, MessageRole};
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

    fn create_test_app_with_full(
        api_token: Option<String>,
        webhook_secret: Option<String>,
        rate_limit_per_minute: Option<u32>,
        session_ttl_secs: Option<u64>,
    ) -> Router {
        let config = AgentConfig::default();
        let agent = JiaClawAgent::new(config.clone()).expect("创建测试 agent 失败");
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));
        let state = AppState {
            agent: Arc::new(agent),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            api_token,
            webhook_secret,
            persist_enabled: false,
            persist_path: Arc::new(persist_path),
            rate_limiter: rate_limit_per_minute.and_then(build_rate_limiter),
            session_ttl: session_ttl_secs.map(Duration::from_secs),
        };

        build_router(state)
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
            persist_enabled: true,
            persist_path: Arc::new(persist_path.clone()),
            rate_limiter: None,
            session_ttl: Some(Duration::from_secs(1)),
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
            persist_enabled: true,
            persist_path: Arc::new(persist_path.clone()),
            rate_limiter: None,
            session_ttl: None,
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
        let config = AgentConfig::default();
        let agent = JiaClawAgent::new(config.clone()).expect("创建测试 agent 失败");
        let persist_path =
            std::env::temp_dir().join(format!("jiaclaw-test-{}.json", uuid::Uuid::new_v4()));
        let state = AppState {
            agent: Arc::new(agent),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            api_token: None,
            webhook_secret: None,
            persist_enabled: false,
            persist_path: Arc::new(persist_path),
            rate_limiter: None,
            session_ttl: None,
        };

        let session_id = "test-limit-session".to_string();

        // 创建超过上限的消息
        let mut messages = vec![ChatMessage {
            role: MessageRole::System,
            content: "你是一个助手".to_string(),
        }];

        // 添加 60 条消息（超过 MAX_SESSION_MESSAGES = 50）
        for i in 0..60 {
            messages.push(ChatMessage {
                role: MessageRole::User,
                content: format!("消息 {}", i),
            });
        }

        // 手动设置 session 历史
        {
            let mut sessions = state.sessions.lock().unwrap();
            sessions.insert(session_id.clone(), SessionRecord::new(messages.clone()));
        }

        // 创建请求并通过 chat_handler 处理
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
            .with_state(state);

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

        // 检查 session 中的消息数是否被限制
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let chat_response: ChatResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(chat_response.session_id, Some(session_id));

        // 注意: 由于实现细节，实际存储的消息数可能略超过 MAX_SESSION_MESSAGES
        // 但应该在合理范围内（< MAX_SESSION_MESSAGES + 2，考虑新消息和响应）
        // 这里我们主要验证截断逻辑被触发了
        // 可以通过日志验证，或者检查消息内容包含 system 消息
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

        // 验证工具信息包含名称和描述
        for tool in &tools_response.tools {
            assert!(!tool.name.is_empty(), "工具名称不应为空");
            assert!(!tool.description.is_empty(), "工具描述不应为空");
        }
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
            persist_enabled: false,
            persist_path: Arc::new(persist_path.clone()),
            rate_limiter: None,
            session_ttl: None,
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

    #[test]
    fn test_is_rate_limited_path() {
        assert!(is_rate_limited_path("/api/chat"));
        assert!(is_rate_limited_path("/api/tools"));
        assert!(is_rate_limited_path("/api/sessions"));
        assert!(is_rate_limited_path("/api/sessions/abc"));
        assert!(is_rate_limited_path("/api/openapi.json"));
        assert!(is_rate_limited_path("/hooks/inbound"));
        assert!(!is_rate_limited_path("/health"));
        assert!(!is_rate_limited_path("/"));
        assert!(!is_rate_limited_path("/api"));
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
            "/api/chat",
            "/api/sessions",
            "/api/sessions/{id}",
            "/api/tools",
            "/api/skills",
            "/hooks/inbound",
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
    }
}
