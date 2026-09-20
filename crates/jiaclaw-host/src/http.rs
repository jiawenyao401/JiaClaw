// Copyright 2026 JiaClaw contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! HTTP 服务模块

use anyhow::{Context, Result};
use axum::{
    extract::{Json, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Router,
};
use jiaclaw::JiaClawAgent;
use jiaclaw_core::{AgentConfig, ChatMessage, ChatRequest, MessageRole};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use tower_http::trace::TraceLayer;

/// Webhook 入站请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookInboundRequest {
    /// 通道类型（固定为 "webhook"）
    #[serde(default = "default_channel")]
    pub channel: String,

    /// 聊天 ID（用于映射 session）
    pub chat_id: String,

    /// 消息文本
    pub text: String,

    /// 用户名（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

fn default_channel() -> String {
    "webhook".to_string()
}

/// Webhook 入站响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookInboundResponse {
    /// 是否成功
    pub ok: bool,

    /// 回复消息
    pub reply: String,

    /// Session ID
    pub session_id: String,

    /// 工具调用列表
    #[serde(default)]
    pub tool_calls: Vec<jiaclaw_core::ToolCall>,
}

/// Session 数据
#[derive(Debug, Clone)]
struct Session {
    /// 会话历史
    messages: Vec<ChatMessage>,
}

impl Session {
    fn new() -> Self {
        Self {
            messages: Vec::new(),
        }
    }

    fn add_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
    }

    fn messages(&self) -> Vec<ChatMessage> {
        self.messages.clone()
    }
}

/// Session 管理器
#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
}

impl SessionManager {
    /// 创建新的 Session 管理器
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn get_or_create(&self, session_id: &str) -> Session {
        let mut sessions = self.sessions.write().unwrap();
        sessions
            .entry(session_id.to_string())
            .or_insert_with(Session::new)
            .clone()
    }

    fn update(&self, session_id: &str, session: Session) {
        let mut sessions = self.sessions.write().unwrap();
        sessions.insert(session_id.to_string(), session);
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 应用状态
#[derive(Clone)]
pub struct AppState {
    /// Agent 实例
    pub agent: Arc<JiaClawAgent>,
    /// Session 管理器
    pub sessions: SessionManager,
    /// Webhook secret（可选）
    pub webhook_secret: Option<String>,
}

/// 处理 Webhook 入站请求
pub async fn handle_webhook_inbound(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WebhookInboundRequest>,
) -> impl IntoResponse {
    // 验证 webhook secret（如果设置）
    if let Some(ref expected_secret) = state.webhook_secret {
        match headers.get("X-Webhook-Secret") {
            Some(header_value) => {
                if header_value.to_str().unwrap_or("") != expected_secret {
                    tracing::warn!("Webhook secret 验证失败");
                    return (
                        StatusCode::UNAUTHORIZED,
                        Json(serde_json::json!({
                            "ok": false,
                            "error": "无效的 webhook secret"
                        })),
                    );
                }
            }
            None => {
                tracing::warn!("缺少 X-Webhook-Secret header");
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "ok": false,
                        "error": "缺少 webhook secret"
                    })),
                );
            }
        }
    }

    // 构建 session_id
    let session_id = format!("webhook:{}", payload.chat_id);

    // 获取或创建 session
    let mut session = state.sessions.get_or_create(&session_id);

    // 添加用户消息到历史
    let user_message = ChatMessage {
        role: MessageRole::User,
        content: payload.text.clone(),
    };
    session.add_message(user_message.clone());

    // 构建聊天请求
    let chat_request = ChatRequest {
        messages: session.messages(),
        enabled_tools: vec![],
        enabled_skills: vec![],
    };

    // 调用 agent
    let response = match state.agent.chat(&chat_request).await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::error!("Agent 调用失败: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": format!("Agent 调用失败: {}", e)
                })),
            );
        }
    };

    // 添加助手消息到历史
    session.add_message(response.message.clone());

    // 更新 session
    state.sessions.update(&session_id, session);

    // 构建响应
    let webhook_response = WebhookInboundResponse {
        ok: true,
        reply: response.message.content.clone(),
        session_id: session_id.clone(),
        tool_calls: response.tool_calls.clone(),
    };

    tracing::info!(
        "Webhook 请求处理成功: session_id={}, reply_len={}",
        session_id,
        response.message.content.len()
    );

    (StatusCode::OK, Json(serde_json::to_value(webhook_response).unwrap()))
}

/// 创建 HTTP 应用
pub fn create_app(config: AgentConfig, webhook_secret: Option<String>) -> Result<Router> {
    let agent = JiaClawAgent::new(config).context("创建 JiaClawAgent 失败")?;

    let state = AppState {
        agent: Arc::new(agent),
        sessions: SessionManager::new(),
        webhook_secret,
    };

    let app = Router::new()
        .route("/hooks/inbound", post(handle_webhook_inbound))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    Ok(app)
}

/// 启动 HTTP 服务器
pub async fn serve(bind: &str, config: AgentConfig) -> Result<()> {
    let webhook_secret = std::env::var("JIACLAW_WEBHOOK_SECRET").ok();

    if webhook_secret.is_some() {
        tracing::info!("Webhook secret 已配置,将验证入站请求");
    } else {
        tracing::warn!("未配置 JIACLAW_WEBHOOK_SECRET,Webhook 端点将不进行鉴权");
    }

    let app = create_app(config, webhook_secret)?;

    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .context(format!("无法绑定到 {}", bind))?;

    tracing::info!("HTTP 服务启动于 http://{}", bind);
    tracing::info!("Webhook 端点: POST http://{}/hooks/inbound", bind);

    axum::serve(listener, app)
        .await
        .context("HTTP 服务运行失败")?;

    Ok(())
}
