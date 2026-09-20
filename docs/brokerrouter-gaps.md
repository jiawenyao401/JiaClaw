# Brokerrouter 能力差距跟踪

本文档跟踪 JiaClaw 集成 [Brokerrouter](https://github.com/StateKnot/Brokerrouter) 时的需求和差距。

**更新时间**: 2026-09-20  
**状态**: Brokerrouter 仓库当前不可访问（404 Not Found）

---

## 概述

根据产品约束，JiaClaw 必须通过 **Brokerrouter** 作为 AI Gateway 路由所有模型/提供商调用，而非直接调用提供商 API。

### 预期架构

```
JiaClaw Agent
    ↓
  Provider Adapter (provider.rs)
    ↓
  Brokerrouter Gateway
    ↓
  Upstream Providers (OpenAI, Anthropic, Ollama, etc.)
```

### 设计原则

1. **适配器边界清晰** - Provider 层抽象化，易于切换实现
2. **离线模式保留** - 存根模式不依赖 Brokerrouter
3. **配置驱动** - Brokerrouter 端点通过配置文件指定
4. **错误透明** - Brokerrouter 错误映射到 JiaClaw 错误类型

---

## Brokerrouter 预期功能

基于 AI Gateway 的标准需求，JiaClaw 期望 Brokerrouter 提供以下能力：

### 1. 基础路由 (P0 - Critical)

**需求**:
- OpenAI-compatible HTTP API (`/v1/chat/completions`)
- 根据配置路由到上游提供商（OpenAI, Anthropic, Ollama, 本地模型）
- 透传标准 OpenAI Chat Completions 请求/响应格式

**JiaClaw 用例**:
```rust
// 配置示例
[agent.provider]
provider_type = "brokerrouter"
base_url = "http://localhost:8090"  # Brokerrouter 端点
model = "gpt-4o-mini"
api_key = "jiaclaw-token"
```

**验收标准**:
- [ ] Brokerrouter 实现 OpenAI-compatible `/v1/chat/completions` 端点
- [ ] 支持 `Authorization: Bearer <token>` 认证
- [ ] 返回标准 OpenAI 响应格式

**议题**: [StateKnot/Brokerrouter#1](https://github.com/StateKnot/Brokerrouter/issues/1) *(待创建)*

---

### 2. 多提供商支持 (P0 - Critical)

**需求**:
- 配置多个上游提供商（OpenAI, Anthropic, Ollama, 自定义端点）
- 根据模型名称自动路由（如 `gpt-4` → OpenAI, `claude-3` → Anthropic）
- 提供商失败时的降级策略（可选）

**JiaClaw 用例**:
```toml
# JiaClaw 配置中指定模型，Brokerrouter 自动路由
model = "gpt-4o-mini"          # 路由到 OpenAI
# model = "claude-3-sonnet"    # 路由到 Anthropic
# model = "llama3"             # 路由到 Ollama
```

**验收标准**:
- [ ] Brokerrouter 配置文件支持多个提供商
- [ ] 根据模型前缀/名称自动路由
- [ ] 路由失败时返回清晰的错误消息

**议题**: [StateKnot/Brokerrouter#2](https://github.com/StateKnot/Brokerrouter/issues/2) *(待创建)*

---

### 3. 认证与授权 (P1 - High)

**需求**:
- API Token 验证（防止未授权访问）
- 租户隔离（多用户场景）
- 可选的速率限制

**JiaClaw 用例**:
- JiaClaw 使用单一 `api_key` 连接 Brokerrouter
- Brokerrouter 验证 token，代理到上游提供商
- Brokerrouter 管理上游 API keys（不暴露给 JiaClaw）

**验收标准**:
- [ ] 支持 Bearer token 认证
- [ ] Token 验证失败返回 401 Unauthorized
- [ ] 上游 API keys 在 Brokerrouter 配置，不通过请求传递

**议题**: [StateKnot/Brokerrouter#3](https://github.com/StateKnot/Brokerrouter/issues/3) *(待创建)*

---

### 4. 流式响应 (P1 - High)

**需求**:
- SSE (Server-Sent Events) 流式响应
- 支持 `stream: true` 参数
- 逐 token 流式输出

**JiaClaw 用例**:
- M4 实现 HTTP/SSE 服务时需要流式响应
- 实时显示 Agent 生成的内容

**验收标准**:
- [ ] 支持 `stream: true` 参数
- [ ] 返回 `data: [DONE]` 结束标记
- [ ] 兼容 OpenAI SSE 格式

**议题**: [StateKnot/Brokerrouter#4](https://github.com/StateKnot/Brokerrouter/issues/4) *(待创建)*

---

### 5. 工具调用支持 (P1 - High)

**需求**:
- 支持 OpenAI Function Calling 格式
- 透传 `tools` 参数和 `tool_choice`
- 处理 `tool_calls` 响应

**JiaClaw 用例**:
- M2 实现工具系统后，需要通过模型调用工具
- 模型返回 `tool_calls`，JiaClaw 执行工具，再次调用模型

**验收标准**:
- [ ] 支持 `tools` 参数（OpenAI 格式）
- [ ] 正确透传和路由 tool calling 请求
- [ ] 处理 `function_call` / `tool_calls` 响应

**议题**: [StateKnot/Brokerrouter#5](https://github.com/StateKnot/Brokerrouter/issues/5) *(待创建)*

---

### 6. 错误处理与重试 (P1 - High)

**需求**:
- 上游提供商错误的透明映射
- 网络失败自动重试（可配置）
- 超时控制

**JiaClaw 用例**:
- 上游 API 返回 429 (Rate Limit) 或 500 (Server Error) 时的处理
- Brokerrouter 自动重试或返回清晰错误

**验收标准**:
- [ ] 透传上游 HTTP 状态码和错误消息
- [ ] 可配置的重试策略（次数、指数退避）
- [ ] 超时配置（请求级和全局级）

**议题**: [StateKnot/Brokerrouter#6](https://github.com/StateKnot/Brokerrouter/issues/6) *(待创建)*

---

### 7. 可观测性 (P2 - Medium)

**需求**:
- 请求日志（请求/响应、延迟、tokens 使用）
- Prometheus 指标导出
- OpenTelemetry 追踪

**JiaClaw 用例**:
- 调试 Agent 行为时需要查看模型调用日志
- 生产环境监控 token 使用和成本

**验收标准**:
- [ ] 结构化日志（JSON 格式）
- [ ] Prometheus `/metrics` 端点
- [ ] 可选的 OpenTelemetry 集成

**议题**: [StateKnot/Brokerrouter#7](https://github.com/StateKnot/Brokerrouter/issues/7) *(待创建)*

---

### 8. 成本控制与预算 (P2 - Medium)

**需求**:
- 每请求/每租户的 token 计数
- 成本估算（基于上游定价）
- 预算上限和告警

**JiaClaw 用例**:
- StateKnot 的预算控制集成
- 防止单次运行消耗过多 tokens

**验收标准**:
- [ ] 记录每次请求的 prompt/completion tokens
- [ ] 根据模型计算成本（可配置定价）
- [ ] 支持预算上限（per-key, per-tenant）

**议题**: [StateKnot/Brokerrouter#8](https://github.com/StateKnot/Brokerrouter/issues/8) *(待创建)*

---

### 9. 缓存 (P2 - Medium)

**需求**:
- 相同请求的响应缓存（降低成本和延迟）
- 可配置的 TTL 和缓存键策略
- 可选的语义缓存（嵌入相似度）

**JiaClaw 用例**:
- 重复的系统提示或工具描述缓存
- 降低开发/测试成本

**验收标准**:
- [ ] 基于请求哈希的缓存
- [ ] 可配置 TTL 和缓存大小
- [ ] 缓存命中/未命中统计

**议题**: [StateKnot/Brokerrouter#9](https://github.com/StateKnot/Brokerrouter/issues/9) *(待创建)*

---

### 10. 配置管理 (P1 - High)

**需求**:
- 配置文件格式（TOML/YAML/JSON）
- 热重载（无需重启）
- 环境变量覆盖

**JiaClaw 用例**:
- Brokerrouter 配置上游提供商、路由规则、认证
- 开发环境快速切换配置

**验收标准**:
- [ ] 支持 TOML 或 YAML 配置文件
- [ ] 配置变更时自动重载（或 SIGHUP）
- [ ] 环境变量优先级高于配置文件

**议题**: [StateKnot/Brokerrouter#10](https://github.com/StateKnot/Brokerrouter/issues/10) *(待创建)*

---

## JiaClaw 集成计划

### 阶段 1: 适配器抽象 (M0.5 - 当前 PR) ✅

- [x] 创建 `Provider` trait 抽象层
- [x] 实现 `StubProvider`（离线模式）
- [x] 实现 `OpenAICompatibleProvider`（直连模式，临时）
- [x] 配置 `ProviderConfig` 支持 `base_url`

### 阶段 2: Brokerrouter 集成 (M1)

**依赖**: Brokerrouter 仓库可访问 + 基础路由实现

- [ ] 克隆 Brokerrouter，阅读 README 和 API 文档
- [ ] 运行 Brokerrouter 本地实例（Docker 或 `cargo run`）
- [ ] 实现 `BrokerrouterProvider`
  - 连接 Brokerrouter 端点（`base_url`）
  - 传递 API token 认证
  - 调用 `/v1/chat/completions`
  - 映射错误响应
- [ ] 更新 `ProviderConfig` 添加 `provider_type = "brokerrouter"`
- [ ] 文档化 Brokerrouter 配置示例
- [ ] 测试：JiaClaw → Brokerrouter → OpenAI/Ollama

### 阶段 3: 高级特性 (M2-M4)

- [ ] 流式响应集成（M4 HTTP 服务）
- [ ] 工具调用透传（M2 工具系统）
- [ ] 错误重试策略配置
- [ ] 可观测性集成（日志、指标）

---

## 临时方案

在 Brokerrouter 可用之前，JiaClaw 保留以下临时路径：

1. **存根模式** (`provider_type = "stub"`) - 无需任何外部服务
2. **直连模式** (`provider_type = "openai_compatible"`) - 直接调用 OpenAI/Ollama API
   - ⚠️ 这是临时方案，将在 Brokerrouter 可用后废弃
   - 用于早期开发和测试

**过渡计划**:
- M0.5: 实现直连模式（当前 PR）
- M1: Brokerrouter 可用后，默认切换到 Brokerrouter
- M2: 废弃直连模式，仅保留 Brokerrouter + 存根

---

## 依赖的 Brokerrouter 议题

| 议题编号 | 标题 | 优先级 | 状态 | 链接 |
|---------|------|-------|------|------|
| #1 | OpenAI-compatible API 实现 | P0 | 待创建 | [StateKnot/Brokerrouter#1](https://github.com/StateKnot/Brokerrouter/issues/1) |
| #2 | 多提供商支持和自动路由 | P0 | 待创建 | [StateKnot/Brokerrouter#2](https://github.com/StateKnot/Brokerrouter/issues/2) |
| #3 | 认证与授权 | P1 | 待创建 | [StateKnot/Brokerrouter#3](https://github.com/StateKnot/Brokerrouter/issues/3) |
| #4 | 流式响应支持 | P1 | 待创建 | [StateKnot/Brokerrouter#4](https://github.com/StateKnot/Brokerrouter/issues/4) |
| #5 | 工具调用支持 | P1 | 待创建 | [StateKnot/Brokerrouter#5](https://github.com/StateKnot/Brokerrouter/issues/5) |
| #6 | 错误处理与重试 | P1 | 待创建 | [StateKnot/Brokerrouter#6](https://github.com/StateKnot/Brokerrouter/issues/6) |
| #7 | 可观测性 | P2 | 待创建 | [StateKnot/Brokerrouter#7](https://github.com/StateKnot/Brokerrouter/issues/7) |
| #8 | 成本控制与预算 | P2 | 待创建 | [StateKnot/Brokerrouter#8](https://github.com/StateKnot/Brokerrouter/issues/8) |
| #9 | 缓存 | P2 | 待创建 | [StateKnot/Brokerrouter#9](https://github.com/StateKnot/Brokerrouter/issues/9) |
| #10 | 配置管理 | P1 | 待创建 | [StateKnot/Brokerrouter#10](https://github.com/StateKnot/Brokerrouter/issues/10) |

**注意**: 议题将在 Brokerrouter 仓库可访问后创建。

---

## 架构原则

### 1. 适配器边界清晰

```rust
// crates/jiaclaw/src/provider.rs

pub trait Provider {
    async fn chat(
        &self,
        model: &str,
        system_prompt: &str,
        messages: &[ChatMessage],
        temperature: f32,
        max_tokens: u32,
    ) -> Result<ChatResponse, JiaClawError>;
}

pub struct StubProvider;          // 离线模式
pub struct BrokerrouterProvider;  // 生产模式（待实现）
pub struct DirectProvider;        // 临时模式（当前实现）
```

### 2. 配置驱动

```toml
# 生产配置（目标）
[agent.provider]
provider_type = "brokerrouter"
base_url = "http://localhost:8090"
api_key = "jiaclaw-token"
model = "gpt-4o-mini"

# 开发配置（临时）
[agent.provider]
provider_type = "openai_compatible"
base_url = "https://api.openai.com/v1"
api_key = "sk-..."
model = "gpt-4o-mini"

# 离线配置
[agent.provider]
provider_type = "stub"
```

### 3. 错误透明

```rust
// Brokerrouter 错误映射到 JiaClawError
match response.status() {
    401 => JiaClawError::Authentication("Invalid API key"),
    429 => JiaClawError::RateLimit("Too many requests"),
    503 => JiaClawError::ServiceUnavailable("Brokerrouter unavailable"),
    _ => JiaClawError::ProviderError(format!("HTTP {}", status)),
}
```

---

## 更新日志

- **2026-09-20**: 初始文档，Brokerrouter 仓库不可访问
- **2026-09-20**: 定义 10 个关键需求和议题占位符

---

## 参考资料

- [Brokerrouter 仓库](https://github.com/StateKnot/Brokerrouter) - 当前 404
- [OpenAI Chat Completions API](https://platform.openai.com/docs/api-reference/chat)
- [JiaClaw 架构文档](architecture.md)
- [StateKnot 能力差距](stateknot-gaps.md)
