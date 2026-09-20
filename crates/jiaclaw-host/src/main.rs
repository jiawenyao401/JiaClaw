// Copyright 2026 JiaClaw contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `JiaClaw` 可执行宿主

use anyhow::{Context, Result};
use axum::{
    extract::{Path, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Json},
    routing::{delete, get, post},
    Router,
};
use clap::{Parser, Subcommand};
use jiaclaw::{JiaClawAgent, Workspace};
use jiaclaw_core::{AgentConfig, ChatMessage, ChatRequest, ChatResponse, MessageRole};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tower_http::cors::{Any, CorsLayer};

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

/// HTTP 服务的共享状态
#[derive(Clone)]
struct AppState {
    agent: Arc<JiaClawAgent>,
    sessions: Arc<Mutex<HashMap<String, Vec<ChatMessage>>>>,
}

/// 健康检查响应
#[derive(Debug, Serialize, Deserialize)]
struct HealthResponse {
    status: String,
    agent_name: String,
    version: String,
}

async fn serve_command(config_path: Option<PathBuf>, bind: String) -> Result<()> {
    tracing::info!("正在启动 JiaClaw Agent 服务于 {}", bind);

    // 加载配置
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

    // 创建 agent
    let agent = JiaClawAgent::new(config).context("创建 JiaClawAgent 失败")?;
    let state = AppState {
        agent: Arc::new(agent),
        sessions: Arc::new(Mutex::new(HashMap::new())),
    };

    // 配置 CORS（允许所有来源，生产环境应该更严格）
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(Any);

    // 构建路由
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/api/chat", post(chat_handler))
        .route("/api/sessions", post(create_session_handler))
        .route("/api/sessions/:id", delete(delete_session_handler))
        .layer(cors)
        .with_state(state);

    // 绑定地址
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .with_context(|| format!("无法绑定到地址: {bind}"))?;

    tracing::info!("✅ HTTP 服务已启动于 http://{}", bind);
    tracing::info!("   • GET    /health              - 健康检查");
    tracing::info!("   • POST   /api/chat            - 聊天端点");
    tracing::info!("   • POST   /api/sessions        - 创建会话");
    tracing::info!("   • DELETE /api/sessions/:id    - 删除会话");
    tracing::info!("\n💡 试试：curl http://{}/health", bind);
    tracing::info!("按 Ctrl+C 停止服务");

    // 启动服务器
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("服务器运行失败")?;

    tracing::info!("服务器已关闭");
    Ok(())
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

/// 聊天处理器
async fn chat_handler(
    State(state): State<AppState>,
    Json(mut request): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, AppError> {
    tracing::info!(
        "收到聊天请求，消息数: {}, session_id: {:?}",
        request.messages.len(),
        request.session_id
    );

    let session_id = request.session_id.clone();

    // 如果提供了 session_id，从 session 中获取历史消息
    if let Some(ref sid) = session_id {
        let sessions = state.sessions.lock().unwrap();
        if let Some(history) = sessions.get(sid) {
            // 将历史消息和新消息合并
            let mut all_messages = history.clone();
            all_messages.extend(request.messages.clone());
            request.messages = all_messages;
            tracing::info!("使用 session {}, 合并后消息数: {}", sid, request.messages.len());
        } else {
            tracing::info!("创建新 session: {}", sid);
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
        sessions.insert(sid.clone(), request.messages.clone());
        sessions.entry(sid.clone()).or_default().push(response.message.clone());
        tracing::info!("更新 session {}, 当前消息数: {}", sid, sessions.get(sid).unwrap().len());
    }

    tracing::info!(
        "聊天响应生成，状态: {:?}, 工具调用数: {}",
        response.status,
        response.tool_calls.len()
    );

    // 将 session_id 添加到响应中
    let mut response = response;
    response.session_id = session_id;

    Ok(Json(response))
}

/// 创建会话响应
#[derive(Debug, Serialize, Deserialize)]
struct CreateSessionResponse {
    session_id: String,
}

/// 创建会话处理器
async fn create_session_handler() -> Json<CreateSessionResponse> {
    let session_id = uuid::Uuid::new_v4().to_string();
    tracing::info!("创建新 session: {}", session_id);
    Json(CreateSessionResponse { session_id })
}

/// 删除会话响应
#[derive(Debug, Serialize, Deserialize)]
struct DeleteSessionResponse {
    success: bool,
    message: String,
}

/// 删除会话处理器
async fn delete_session_handler(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Json<DeleteSessionResponse> {
    let mut sessions = state.sessions.lock().unwrap();
    let existed = sessions.remove(&session_id).is_some();

    if existed {
        tracing::info!("删除 session: {}", session_id);
        Json(DeleteSessionResponse {
            success: true,
            message: format!("会话 {session_id} 已删除"),
        })
    } else {
        tracing::warn!("尝试删除不存在的 session: {}", session_id);
        Json(DeleteSessionResponse {
            success: false,
            message: format!("会话 {session_id} 不存在"),
        })
    }
}

/// 应用错误类型
#[derive(Debug)]
enum AppError {
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            Self::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = serde_json::json!({
            "error": message,
        });

        (status, Json(body)).into_response()
    }
}

/// 优雅关闭信号
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("等待 Ctrl+C 信号失败");
    tracing::info!("收到关闭信号，正在停止服务器...");
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
        session_id: None,
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
        let config = AgentConfig::default();
        let agent = JiaClawAgent::new(config).expect("创建测试 agent 失败");
        let state = AppState {
            agent: Arc::new(agent),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        };

        Router::new()
            .route("/health", get(health_handler))
            .route("/api/chat", post(chat_handler))
            .route("/api/sessions", post(create_session_handler))
            .route("/api/sessions/:id", delete(delete_session_handler))
            .with_state(state)
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = create_test_app();

        let response = app
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
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
        assert!(
            !chat_response.tool_calls.is_empty(),
            "应该有工具调用"
        );
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
}
