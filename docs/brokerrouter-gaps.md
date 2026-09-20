# Brokerrouter 能力差距跟踪（JiaClaw 消费方）

> 更新：Brokerrouter 仓库为 **private**（不是缺失）。已具备 `POST /v1/chat/completions`（text-nonstream-v1）。
> JiaClaw 强制经 Brokerrouter 作为 AI Gateway；以下议题针对**真实契约**而非臆测 404。

## 已创建的上游议题

| 优先级 | 议题 | 链接 |
|--------|------|------|
| P0 | 消费方接入指南（虚拟密钥 / Idempotency-Key） | https://github.com/StateKnot/Brokerrouter/issues/28 |
| P1 | Chat SSE `stream:true` | https://github.com/StateKnot/Brokerrouter/issues/29 |
| P0 | 单人本地 personal/dev 一键配置 | https://github.com/StateKnot/Brokerrouter/issues/30 |
| P1 | Agent tool roundtrip 认证清单 | https://github.com/StateKnot/Brokerrouter/issues/31 |

详见各 issue 正文。

## 当前集成策略

1. **推荐路径**：`provider_type = "brokerrouter"` → `{base_url}/v1/chat/completions`，Bearer 虚拟密钥，每次尝试生成 `Idempotency-Key`。
2. **临时路径**：`openai_compatible` 直连（仅开发逃生舱，目标废弃）。
3. **离线路径**：`stub` 永久保留。

## 已知契约要点（来自 Brokerrouter M2）

- 支持非流式 chat + function tools（协议层）
- 拒绝 `stream:true`、多模态 content、`n>1` 等
- 收费 POST 必须带幂等键
- 人民币账本 / 虚拟密钥 / 租户模型

## 集成状态

✅ **已完成**：
- `BrokerrouterProvider` 实现（`crates/jiaclaw/src/provider/brokerrouter.rs`）
- Bearer 虚拟密钥认证
- 自动幂等性密钥生成（`jiaclaw-{UUID}`，1-200 可打印 ASCII）
- 非流式聊天补全（`stream: false`）
- 请求追踪（捕获 `x-request-id` / `x-brokerrouter-request-id`）
- HTTP 模拟测试（wiremock）
- 错误处理和状态码映射
- 配置示例更新（推荐 Brokerrouter）

## 使用示例

### TOML 配置（推荐）

```toml
[provider]
type = "brokerrouter"
base_url = "https://api.brokerrouter.dev"
api_key = "brk_live_..."  # 或使用环境变量 JIACLAW_API_KEY
model = "claude-3-5-sonnet-20241022"
temperature = 0.7
max_tokens = 4096
```

### 环境变量

```bash
export JIACLAW_API_KEY=brk_live_...
jiaclaw chat "你好"
```

## 测试

```bash
# 运行所有测试
cargo test

# 运行 Brokerrouter 特定测试
cargo test --package jiaclaw brokerrouter

# 运行 clippy 检查
cargo clippy -- -D warnings
```
