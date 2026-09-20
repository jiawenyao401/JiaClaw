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
