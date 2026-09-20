// Copyright 2026 JiaClaw contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Webhook 入站端点测试

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use jiaclaw_core::AgentConfig;
use serde_json::json;
use tower::ServiceExt as _;

mod common {
    use super::*;
    use jiaclaw_host::http::create_app;

    pub fn test_config() -> AgentConfig {
        AgentConfig::default()
    }

    pub async fn app_without_secret() -> axum::Router {
        create_app(test_config(), None).expect("Failed to create app")
    }

    pub async fn app_with_secret(secret: &str) -> axum::Router {
        create_app(test_config(), Some(secret.to_string())).expect("Failed to create app")
    }
}

#[tokio::test]
async fn test_webhook_inbound_success_without_secret() {
    let app = common::app_without_secret().await;

    let payload = json!({
        "chat_id": "test-chat-123",
        "text": "你好",
        "username": "test_user"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/hooks/inbound")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let response_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(response_json["ok"], true);
    assert!(response_json["reply"].is_string());
    assert_eq!(response_json["session_id"], "webhook:test-chat-123");
    assert!(response_json["tool_calls"].is_array());
}

#[tokio::test]
async fn test_webhook_inbound_missing_secret() {
    let app = common::app_with_secret("my-secret-token").await;

    let payload = json!({
        "chat_id": "test-chat-456",
        "text": "你好"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/hooks/inbound")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let response_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(response_json["ok"], false);
    assert!(response_json["error"].as_str().unwrap().contains("secret"));
}

#[tokio::test]
async fn test_webhook_inbound_invalid_secret() {
    let app = common::app_with_secret("my-secret-token").await;

    let payload = json!({
        "chat_id": "test-chat-789",
        "text": "你好"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/hooks/inbound")
                .header("content-type", "application/json")
                .header("X-Webhook-Secret", "wrong-secret")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_webhook_inbound_valid_secret() {
    let app = common::app_with_secret("my-secret-token").await;

    let payload = json!({
        "chat_id": "test-chat-abc",
        "text": "你好"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/hooks/inbound")
                .header("content-type", "application/json")
                .header("X-Webhook-Secret", "my-secret-token")
                .body(Body::from(serde_json::to_string(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let response_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(response_json["ok"], true);
    assert_eq!(response_json["session_id"], "webhook:test-chat-abc");
}

#[tokio::test]
async fn test_webhook_session_persistence() {
    let app = common::app_without_secret().await;

    // 第一条消息
    let payload1 = json!({
        "chat_id": "session-test",
        "text": "第一条消息"
    });

    let response1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/hooks/inbound")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload1).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response1.status(), StatusCode::OK);

    // 第二条消息（同一 session）
    let payload2 = json!({
        "chat_id": "session-test",
        "text": "第二条消息"
    });

    let response2 = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/hooks/inbound")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&payload2).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response2.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response2.into_body(), usize::MAX)
        .await
        .unwrap();
    let response_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(response_json["session_id"], "webhook:session-test");
}
